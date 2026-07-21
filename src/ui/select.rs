//! 下拉选择控件 Dropdown：显示当前选项 + 点击弹出浮层列表选择。
//!
//! 复用宿主层浮层机制（与右键菜单同源）：点击经 `EventCtx::show_menu` 请求弹出，
//! 每个选项的动作是设置绑定的 `Rc<Cell<usize>>` 选中索引（`MenuAction::Run` 闭包）。
//!
//! 富内容（副标题/徽章/可点击尾随图标）走 [`DropdownItem`] + `with_items`/`with_items_reactive`；
//! 纯文本场景仍用原有 `Vec<String>` 入口，两者内部分别存储、互不影响。

use std::cell::Cell;
use std::rc::Rc;

use crate::anim::{Easing, Transition};
use crate::core::{EventCtx, Widget};
use crate::event::{Event, Key, MenuItem, PointerKind};
use crate::geometry::{Color, Rect, Size};
use crate::render::{Canvas, Paint};
use crate::signal::Signal;
use crate::spec::Align;
use crate::style::Style;
use crate::text::TextEngine;
use crate::theme::Intent;

const PAD_X: i32 = 12;
const CHEVRON_W: i32 = 18;
/// 收起态徽章胶囊左右内边距/高度/与文本间距（与 `app.rs` 菜单尾随徽章同规格）。
const BADGE_PAD_X: i32 = 8;
const BADGE_H: i32 = 20;
const BADGE_GAP: i32 = 8;

/// 富内容选项：主文本 + 可选第二行说明 + 可选尾随徽章（纯展示）+ 可选尾随可点击图标。
#[derive(Clone)]
pub struct DropdownItem {
    pub label: String,
    pub subtitle: Option<String>,
    pub badge: Option<(String, Intent)>,
    pub trailing_icon: Option<(String, Rc<dyn Fn()>)>,
}

impl DropdownItem {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            subtitle: None,
            badge: None,
            trailing_icon: None,
        }
    }
    /// 第二行小字说明（展开态渲染为两行）。
    pub fn subtitle(mut self, s: impl Into<String>) -> Self {
        self.subtitle = Some(s.into());
        self
    }
    /// 尾随徽章胶囊（纯展示，展开态与收起态当前项均显示）。
    pub fn badge(mut self, text: impl Into<String>, intent: Intent) -> Self {
        self.badge = Some((text.into(), intent));
        self
    }
    /// 尾随可独立点击的图标（仅展开态列表项）：点击只触发 `on_click`，不选中该项。
    pub fn trailing_icon(mut self, icon: impl Into<String>, on_click: impl Fn() + 'static) -> Self {
        self.trailing_icon = Some((icon.into(), Rc::new(on_click)));
        self
    }
}

impl From<String> for DropdownItem {
    fn from(s: String) -> Self {
        Self::new(s)
    }
}
impl From<&str> for DropdownItem {
    fn from(s: &str) -> Self {
        Self::new(s)
    }
}

/// 选项存储：纯文本（原有 `Vec<String>` 入口）或富内容（`DropdownItem`）。
enum OptionSource {
    Plain(Signal<Vec<String>>),
    Rich(Signal<Vec<DropdownItem>>),
}

pub struct Dropdown {
    options: OptionSource,
    selected: Signal<usize>,
    hover: bool,
    /// 边框色补间（hover/focus 高亮淡变）；首帧靠 `primed` 落定。
    border_anim: Cell<Transition<Color>>,
    primed: Cell<bool>,
}

impl Dropdown {
    pub fn new(options: Vec<String>, selected: Signal<usize>) -> Self {
        Self::with_plain_signal(crate::signal::signal(options), selected)
    }

    /// 响应式选项：选项列表绑定外部 `Signal<Vec<String>>`，变更即重新测量/渲染。
    pub fn new_reactive(options: Signal<Vec<String>>, selected: Signal<usize>) -> Self {
        Self::with_plain_signal(options, selected)
    }

    /// 富内容选项（副标题/徽章/尾随图标）。
    pub fn with_items(items: Vec<DropdownItem>, selected: Signal<usize>) -> Self {
        Self::with_rich_signal(crate::signal::signal(items), selected)
    }

    /// 响应式富内容选项：绑定外部 `Signal<Vec<DropdownItem>>`。
    pub fn with_items_reactive(items: Signal<Vec<DropdownItem>>, selected: Signal<usize>) -> Self {
        Self::with_rich_signal(items, selected)
    }

    fn with_plain_signal(options: Signal<Vec<String>>, selected: Signal<usize>) -> Self {
        Self {
            options: OptionSource::Plain(options),
            selected,
            hover: false,
            border_anim: Cell::new(Transition::new(Color::rgba(0, 0, 0, 0))),
            primed: Cell::new(false),
        }
    }

    fn with_rich_signal(options: Signal<Vec<DropdownItem>>, selected: Signal<usize>) -> Self {
        Self {
            options: OptionSource::Rich(options),
            selected,
            hover: false,
            border_anim: Cell::new(Transition::new(Color::rgba(0, 0, 0, 0))),
            primed: Cell::new(false),
        }
    }

    fn current(&self) -> String {
        match &self.options {
            OptionSource::Plain(opts) => opts.with(|list| {
                let i = self.selected.get().min(list.len().saturating_sub(1));
                list.get(i).cloned().unwrap_or_default()
            }),
            OptionSource::Rich(items) => items.with(|list| {
                let i = self.selected.get().min(list.len().saturating_sub(1));
                list.get(i).map(|it| it.label.clone()).unwrap_or_default()
            }),
        }
    }

    /// 当前选中项的尾随徽章（仅富内容来源；纯文本来源恒为 `None`）。
    fn current_badge(&self) -> Option<(String, Intent)> {
        match &self.options {
            OptionSource::Plain(_) => None,
            OptionSource::Rich(items) => items.with(|list| {
                let i = self.selected.get().min(list.len().saturating_sub(1));
                list.get(i).and_then(|it| it.badge.clone())
            }),
        }
    }

    /// 弹出浮层列表：宽度对齐控件，每项点击设置选中索引。
    fn open(&self, ctx: &mut EventCtx) {
        let b = ctx.bounds();
        let cur = self.selected.get();
        let items: Vec<MenuItem> = match &self.options {
            OptionSource::Plain(opts) => {
                let list = opts.get();
                if list.is_empty() {
                    return;
                }
                list.into_iter()
                    .enumerate()
                    .map(|(i, o)| {
                        let sel = self.selected;
                        MenuItem::run(o, move || sel.set(i), i == cur)
                    })
                    .collect()
            }
            OptionSource::Rich(items_sig) => {
                let list = items_sig.get();
                if list.is_empty() {
                    return;
                }
                list.into_iter()
                    .enumerate()
                    .map(|(i, it)| {
                        let sel = self.selected;
                        let mut mi = MenuItem::run(it.label, move || sel.set(i), i == cur);
                        if let Some(sub) = it.subtitle {
                            mi = mi.with_subtitle(sub);
                        }
                        if let Some((text, intent)) = it.badge {
                            mi = mi.with_badge(text, intent);
                        }
                        if let Some((icon, cb)) = it.trailing_icon {
                            mi = mi.with_trailing_icon(icon, move || (*cb)());
                        }
                        mi
                    })
                    .collect()
            }
        };
        ctx.show_dropdown_menu(b, items);
    }
}

impl Widget for Dropdown {
    fn measure(&self, _avail: Size, style: &Style, text: &mut dyn TextEngine) -> Size {
        let mut w = 0;
        match &self.options {
            OptionSource::Plain(opts) => opts.with(|list| {
                for o in list {
                    w = w.max(text.measure(o, &crate::text::TextStyle::of(style), None).w);
                }
            }),
            OptionSource::Rich(items) => items.with(|list| {
                for it in list {
                    let mut iw = text
                        .measure(&it.label, &crate::text::TextStyle::of(style), None)
                        .w;
                    if let Some((btext, _)) = &it.badge {
                        iw += text
                            .measure(btext, &crate::text::TextStyle::new(12.0), None)
                            .w
                            + 2 * BADGE_PAD_X
                            + BADGE_GAP;
                    }
                    w = w.max(iw);
                }
            }),
        }
        Size::new(w + 2 * PAD_X + CHEVRON_W, (style.font_size as i32) + 16)
    }

    fn paint(
        &self,
        bounds: Rect,
        _content: Rect,
        focused: bool,
        enabled: bool,
        canvas: &mut dyn Canvas,
        style: &Style,
    ) {
        let th = crate::theme::current();
        let (pal, dd) = (&th.palette, &th.dropdown);
        let (x, y, w, h) = (
            bounds.x as f32,
            bounds.y as f32,
            bounds.w as f32,
            bounds.h as f32,
        );
        let corner = dd.corner(&th.metrics);
        // 禁用：背景弱化、文字与箭头用 text_disabled。
        let bg = if enabled { dd.bg(pal) } else { pal.surface_alt };
        let text_color = if enabled {
            dd.text(pal)
        } else {
            pal.text_disabled
        };
        let chevron = if enabled {
            dd.chevron(pal)
        } else {
            pal.text_disabled
        };
        canvas.fill_round_rect(x, y, w, h, corner, &Paint::fill(bg));
        // 边框色补间：hover/focus 高亮淡变；首帧落定。
        let target_border = if focused || self.hover {
            dd.border_focus(pal)
        } else {
            dd.border(pal)
        };
        let mut ba = self.border_anim.get();
        if !self.primed.get() {
            ba = Transition::new(target_border);
            self.primed.set(true);
        } else if ba.target() != target_border {
            ba.retarget(target_border, th.anim.fast(), Easing::EaseOut);
        }
        let border = ba.animate();
        self.border_anim.set(ba);
        let bw = if focused {
            th.metrics.border_width_focus.to_logical(canvas.dpi_scale())
        } else {
            th.metrics.border_width.to_logical(canvas.dpi_scale())
        };
        canvas.stroke_round_rect(x, y, w, h, corner, bw, &Paint::fill(border));

        // 当前选中项的尾随徽章（若有）：贴 chevron 左侧，文本区相应收窄。
        let badge = self.current_badge();
        let badge_w = badge
            .as_ref()
            .map(|(text, _)| {
                canvas
                    .measure_text(text, &crate::text::TextStyle::new(12.0))
                    .w
                    + 2 * BADGE_PAD_X
            })
            .unwrap_or(0);
        if let Some((text, intent)) = &badge {
            let (fill, fg) = intent.badge_colors(pal);
            let br = Rect::new(
                bounds.x + bounds.w - PAD_X - CHEVRON_W - badge_w,
                bounds.y + (bounds.h - BADGE_H) / 2,
                badge_w,
                BADGE_H,
            );
            canvas.fill_round_rect(
                br.x as f32,
                br.y as f32,
                br.w as f32,
                br.h as f32,
                999.0,
                &Paint::fill(fill),
            );
            canvas.draw_text(
                text,
                br,
                fg,
                Align::Center,
                &crate::text::TextStyle::new(12.0),
            );
        }

        // 当前选项文本（左侧，留出右侧 chevron 与徽章）。
        let badge_reserve = if badge_w > 0 { badge_w + BADGE_GAP } else { 0 };
        let tr = Rect::new(
            bounds.x + PAD_X,
            bounds.y,
            bounds.w - 2 * PAD_X - CHEVRON_W - badge_reserve,
            bounds.h,
        );
        let cur = self.current();
        canvas.draw_text(
            &cur,
            tr,
            text_color,
            Align::Start,
            &crate::text::TextStyle::of(style),
        );

        // 右侧下拉箭头 ▼（两段线）。
        let cx = bounds.x as f32 + bounds.w as f32 - PAD_X as f32 - CHEVRON_W as f32 / 2.0;
        let cy = bounds.y as f32 + bounds.h as f32 / 2.0;
        let p = Paint::fill(chevron);
        canvas.draw_line(cx - 4.0, cy - 2.0, cx, cy + 3.0, 1.6, &p);
        canvas.draw_line(cx, cy + 3.0, cx + 4.0, cy - 2.0, 1.6, &p);
    }

    fn on_event(&mut self, ctx: &mut EventCtx, ev: &Event) -> bool {
        match ev {
            Event::Pointer(p) => match p.kind {
                PointerKind::Enter => {
                    self.hover = true;
                    ctx.mark_dirty();
                    true
                }
                PointerKind::Leave => {
                    self.hover = false;
                    ctx.mark_dirty();
                    true
                }
                PointerKind::Down => {
                    ctx.request_focus();
                    true
                }
                PointerKind::Up => {
                    if ctx.bounds().contains(p.pos) {
                        // 打开后宿主独占指针，控件收不到 Leave；提前清 hover 避免边框残留。
                        self.hover = false;
                        self.open(ctx);
                    }
                    true
                }
                _ => false,
            },
            Event::Key(k) if k.pressed => match k.key {
                Key::Enter | Key::Space | Key::Down => {
                    self.open(ctx);
                    true
                }
                _ => false,
            },
            _ => false,
        }
    }

    fn focusable(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signal::signal;

    #[test]
    fn reactive_dropdown_reflects_option_signal() {
        let opts = signal(vec!["甲".to_string(), "乙".to_string()]);
        let sel = signal(1usize);
        let dd = Dropdown::new_reactive(opts, sel);
        assert_eq!(dd.current(), "乙");
        // 选项异步更新后，current 立即反映新列表（按同一索引）。
        opts.set(vec!["X".to_string(), "Y".to_string(), "Z".to_string()]);
        assert_eq!(dd.current(), "Y");
        sel.set(2);
        assert_eq!(dd.current(), "Z");
    }

    #[test]
    fn dropdown_current_clamps_when_index_overflows() {
        let opts = signal(vec!["a".to_string(), "b".to_string()]);
        let sel = signal(5usize); // 越界
        let dd = Dropdown::new_reactive(opts, sel);
        assert_eq!(dd.current(), "b"); // 钳到末项
        opts.set(vec![]); // 空列表
        assert_eq!(dd.current(), ""); // 不 panic，返回空
    }

    #[test]
    fn measure_empty_list_is_chrome_only_width() {
        use crate::text::NullTextEngine;
        let dd = Dropdown::new_reactive(signal(vec![]), signal(0usize));
        let style = Style::default();
        let mut te = NullTextEngine;
        // 空列表：宽度仅为左右内边距 + 箭头区（无选项文本贡献）。
        let w = dd.measure(Size::ZERO, &style, &mut te).w;
        assert_eq!(w, 2 * PAD_X + CHEVRON_W);
    }
}

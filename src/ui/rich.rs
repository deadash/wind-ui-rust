//! 轻量富文本控件 `RichText`。
//!
//! 定位：词典条目这类「一段内容里混多种字号/字重/颜色/胶囊标签」的静态排版，
//! 用一个控件 + span 数据模型解决，而非拼接大量 Label。设计要点：
//!
//! - **数据模型**：`RichDoc` = 块（段落 / 分隔线 / 可折叠 Section）的树；段落由
//!   `RichSpan` 组成，样式为全 Option 覆盖（None 继承控件 `Style`）。命名样式表
//!   （[`RichDoc::style`]）让调用方只标语义（"headword"/"pos"…），视觉集中定义。
//! - **布局**：控件自做行内流式布局（与多行 TextInput 的"自绘视觉行"同一范式）：
//!   span 切碎片（Latin 按空格、CJK 逐字可断、`\n` 强制换行），贪心装行；
//!   同行混字号靠 [`TextEngine::line_metrics`] 基线对齐——行基线 = max(各碎片
//!   ascent)，碎片矩形 top = 基线 − 自身 ascent、高 = 自然行高，引擎"矩形内垂直
//!   居中"的绘制约定在矩形高恰为自然行高时退化为顶对齐，字形落在正确基线。
//! - **折叠**：Section 的展开态是 `Signal<bool>`（状态与文档分离，翻转不失效
//!   碎片测量缓存）；折叠 = 布局器不下钻子块——不产出碎片即不测量、不绘制、
//!   不命中。头部整行可点击，悬停手型光标。
//! - **主题**：颜色用 [`RichColor`] 语义角色（paint 时按当前 palette 解析，
//!   运行时换主题自动跟随）或固定色；控件自身 chrome（箭头/分隔线/chip 默认色/
//!   间距）走 `RichTheme` 覆盖层。
//!
//! 已知限制（后续分期）：span 级点击/悬停（词典交叉引用）、行数截断 clamp、
//! 划选复制、展开高度动画、CJK 避头尾。

use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use crate::core::{EventCtx, Widget};
use crate::event::{CursorShape, Event, PointerKind};
use crate::geometry::{Color, Point, Rect, Size};
use crate::render::{Canvas, Paint};
use crate::signal::Signal;
use crate::style::Style;
use crate::text::{LineMetrics, TextEngine, TextStyle};
use crate::theme::{Palette, Theme};

// ---------------------------------------------------------------------------
// 数据模型
// ---------------------------------------------------------------------------

/// 富文本颜色：主题语义角色（paint 时按当前 palette 解析，换主题自动跟随）
/// 或固定色。`Color` 可经 `From` 直接传入。
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RichColor {
    /// 正文色（palette.text）。
    Text,
    /// 次要文字（palette.text_muted）。
    Muted,
    /// 强调色（palette.accent）。
    Accent,
    /// 危险色（palette.danger）。
    Danger,
    /// 固定颜色（不随主题变化）。
    Fixed(Color),
}

impl RichColor {
    fn resolve(self, p: &Palette) -> Color {
        match self {
            RichColor::Text => p.text,
            RichColor::Muted => p.text_muted,
            RichColor::Accent => p.accent,
            RichColor::Danger => p.danger,
            RichColor::Fixed(c) => c,
        }
    }
}

impl From<Color> for RichColor {
    fn from(c: Color) -> Self {
        RichColor::Fixed(c)
    }
}

/// span 样式：全 Option 覆盖，`None` 继承控件 `Style` / 主题默认。
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SpanStyle {
    size: Option<f32>,
    weight: Option<u16>,
    family: Option<String>,
    fg: Option<RichColor>,
    bg: Option<RichColor>,
    underline: bool,
    strike: bool,
    chip: bool,
}

impl SpanStyle {
    pub fn new() -> Self {
        Self::default()
    }
    /// 字号（逻辑 dp）。
    pub fn size(mut self, size: f32) -> Self {
        self.size = Some(size);
        self
    }
    /// 字重（400 常规 / 600 半粗 / 700 粗）。
    pub fn weight(mut self, weight: u16) -> Self {
        self.weight = Some(weight);
        self
    }
    /// 粗体（weight 700）。
    pub fn bold(self) -> Self {
        self.weight(700)
    }
    /// 字族（音标等特殊字体场景）。
    pub fn family(mut self, family: impl Into<String>) -> Self {
        self.family = Some(family.into());
        self
    }
    /// 前景色（语义角色或固定色）。
    pub fn fg(mut self, color: impl Into<RichColor>) -> Self {
        self.fg = Some(color.into());
        self
    }
    /// 背景色。非 chip 时为文字底色高亮；chip 时为胶囊底色。
    pub fn bg(mut self, color: impl Into<RichColor>) -> Self {
        self.bg = Some(color.into());
        self
    }
    /// 下划线。
    pub fn underline(mut self) -> Self {
        self.underline = true;
        self
    }
    /// 删除线。
    pub fn strike(mut self) -> Self {
        self.strike = true;
        self
    }
    /// 胶囊（pill）：加内边距 + 全圆角底色，整体不随行拆分。
    /// 词性标签、领域标签等即此。未指定 fg/bg 时用 `RichTheme` 的 chip 默认色。
    pub fn chip(mut self) -> Self {
        self.chip = true;
        self
    }

    /// 以 `base` 为底、本样式的显式字段覆盖（命名样式 + 内联覆盖的合并规则）。
    fn over(&self, base: &SpanStyle) -> SpanStyle {
        SpanStyle {
            size: self.size.or(base.size),
            weight: self.weight.or(base.weight),
            family: self.family.clone().or_else(|| base.family.clone()),
            fg: self.fg.or(base.fg),
            bg: self.bg.or(base.bg),
            underline: self.underline || base.underline,
            strike: self.strike || base.strike,
            chip: self.chip || base.chip,
        }
    }
}

/// 一段带样式的文字。经 [`Para`] 的 builder 方法构造。
#[derive(Clone, Debug)]
struct RichSpan {
    text: String,
    /// 命名样式（在 `RichDoc` 样式表中查找作为基底）。
    named: Option<String>,
    /// 内联样式（覆盖命名样式的对应字段）。
    style: SpanStyle,
}

/// 段落：span 序列 + 段级排版参数。
#[derive(Clone, Debug, Default)]
pub struct Para {
    spans: Vec<RichSpan>,
    /// 段首行缩进（逻辑 px，相对当前块缩进基线）。
    indent: i32,
}

impl Para {
    pub fn new() -> Self {
        Self::default()
    }
    /// 默认样式文字。
    pub fn text(mut self, s: impl Into<String>) -> Self {
        self.spans.push(RichSpan {
            text: s.into(),
            named: None,
            style: SpanStyle::default(),
        });
        self
    }
    /// 命名样式文字（样式名在 [`RichDoc::style`] 注册）。
    pub fn styled(mut self, name: impl Into<String>, s: impl Into<String>) -> Self {
        self.spans.push(RichSpan {
            text: s.into(),
            named: Some(name.into()),
            style: SpanStyle::default(),
        });
        self
    }
    /// 内联样式文字。
    pub fn span(mut self, s: impl Into<String>, style: SpanStyle) -> Self {
        self.spans.push(RichSpan {
            text: s.into(),
            named: None,
            style,
        });
        self
    }
    /// 命名样式 + 内联覆盖（内联显式字段优先）。
    pub fn styled_span(
        mut self,
        name: impl Into<String>,
        s: impl Into<String>,
        style: SpanStyle,
    ) -> Self {
        self.spans.push(RichSpan {
            text: s.into(),
            named: Some(name.into()),
            style,
        });
        self
    }
    /// 段缩进（逻辑 px）。
    pub fn indent(mut self, px: i32) -> Self {
        self.indent = px;
        self
    }
}

impl From<&str> for Para {
    fn from(s: &str) -> Self {
        Para::new().text(s)
    }
}
impl From<String> for Para {
    fn from(s: String) -> Self {
        Para::new().text(s)
    }
}

/// 块：段落 / 分隔线 / 可折叠 Section。
#[derive(Clone)]
enum RichBlock {
    Para(Para),
    Divider,
    Section(Section),
}

/// 可折叠区：头部（自动加折叠箭头）+ 子块（折叠时不参与布局）。
#[derive(Clone)]
struct Section {
    header: Para,
    children: Vec<RichBlock>,
    collapsed: Signal<bool>,
}

/// 富文本文档：块序列 + 命名样式表。经 builder 构造后交给 [`super::Element::rich`]。
#[derive(Clone, Default)]
pub struct RichDoc {
    blocks: Vec<RichBlock>,
    styles: HashMap<String, SpanStyle>,
}

impl RichDoc {
    pub fn new() -> Self {
        Self::default()
    }
    /// 注册命名样式（语义样式表）。span 经 [`Para::styled`] 引用；
    /// 未注册的名字按默认样式处理。
    pub fn style(mut self, name: impl Into<String>, style: SpanStyle) -> Self {
        self.styles.insert(name.into(), style);
        self
    }
    /// 追加一个段落（`&str` 可直接传入成为单 span 段落）。
    pub fn para(mut self, p: impl Into<Para>) -> Self {
        self.blocks.push(RichBlock::Para(p.into()));
        self
    }
    /// 追加一条分隔线（义项之间的细线，宽度撑满控件）。
    pub fn divider(mut self) -> Self {
        self.blocks.push(RichBlock::Divider);
        self
    }
    /// 追加一个可折叠区。`collapsed` 为展开态信号（true = 收起），头部点击自动翻转；
    /// 子块经嵌套 builder 构造（其命名样式并入本文档，同名后定义覆盖）。
    pub fn section(
        mut self,
        header: impl Into<Para>,
        collapsed: Signal<bool>,
        children: impl FnOnce(RichDoc) -> RichDoc,
    ) -> Self {
        let inner = children(RichDoc::new());
        self.styles.extend(inner.styles);
        self.blocks.push(RichBlock::Section(Section {
            header: header.into(),
            children: inner.blocks,
            collapsed,
        }));
        self
    }
}

// ---------------------------------------------------------------------------
// 布局
// ---------------------------------------------------------------------------

/// 测量抽象：布局算法同时服务 measure（`TextEngine`）与 paint（`Canvas`）两条路径。
trait Measurer {
    fn size(&mut self, text: &str, ts: &TextStyle) -> Size;
    fn metrics(&mut self, text: &str, ts: &TextStyle) -> LineMetrics;
}

struct EngineMeasurer<'a>(&'a mut dyn TextEngine);
impl Measurer for EngineMeasurer<'_> {
    fn size(&mut self, text: &str, ts: &TextStyle) -> Size {
        self.0.measure(text, ts, None)
    }
    fn metrics(&mut self, text: &str, ts: &TextStyle) -> LineMetrics {
        self.0.line_metrics(text, ts)
    }
}

struct CanvasMeasurer<'a>(&'a mut dyn Canvas);
impl Measurer for CanvasMeasurer<'_> {
    fn size(&mut self, text: &str, ts: &TextStyle) -> Size {
        self.0.measure_text(text, ts)
    }
    fn metrics(&mut self, text: &str, ts: &TextStyle) -> LineMetrics {
        self.0.text_line_metrics(text, ts)
    }
}

/// 碎片的已解析样式（family 已合并控件 Style，颜色保留语义角色供 paint 时解析）。
#[derive(Clone, Debug)]
struct FragStyle {
    size: f32,
    weight: u16,
    family: Option<String>,
    line_height: Option<f32>,
    fg: Option<RichColor>,
    bg: Option<RichColor>,
    underline: bool,
    strike: bool,
    chip: bool,
}

impl FragStyle {
    fn ts(&self) -> TextStyle<'_> {
        TextStyle {
            family: self.family.as_deref(),
            size: self.size,
            weight: self.weight,
            line_height: self.line_height,
        }
    }
}

/// 已排版碎片。坐标相对控件 content 左上角（逻辑 px）。
#[derive(Debug)]
struct Frag {
    text: String,
    /// 碎片全框（chip 含内边距）。
    rect: Rect,
    /// 文字矩形：高恰为该文字自然行高——引擎"矩形内垂直居中"即顶对齐，落在基线上。
    text_rect: Rect,
    /// 基线距 text_rect 顶（画下划线/删除线用）。
    ascent: f32,
    style: FragStyle,
    /// Section 折叠箭头（fg 走 RichTheme.chevron）。
    chevron: bool,
}

/// 布局缓存键。任何影响几何的输入都在此；颜色不在（paint 时解析，换主题不重排）。
#[derive(Clone, PartialEq, Debug)]
struct LayoutKey {
    wrap_w: Option<i32>,
    family: Option<String>,
    size_bits: u32,
    weight: u16,
    line_height_bits: Option<u32>,
    /// 各 Section 折叠态快照（文档序）。
    collapsed: Vec<bool>,
    /// 主题间距参数（para_spacing, section_indent）。
    spacing: (i32, i32),
}

/// 布局产物。
struct RichLayout {
    frags: Vec<Frag>,
    /// 折叠头命中区（相对 content；宽度撑满可用宽）+ 对应折叠信号。
    headers: Vec<(Rect, Signal<bool>)>,
    /// 分隔线 (x缩进, y)；绘制时延展到 content 右缘。
    dividers: Vec<(i32, i32)>,
    /// 自然尺寸（最宽行 × 总高）。
    size: Size,
    key: LayoutKey,
}

/// 行内待排项（碎片测量结果 + 盒参数）。
struct Item {
    text: String,
    style: FragStyle,
    chevron: bool,
    /// 空白碎片：行首丢弃、不触发换行、无视觉时不产出 Frag。
    space: bool,
    text_w: i32,
    text_h: i32,
    ascent: f32,
    /// chip 内边距 (横, 纵)；非 chip 为 0。
    pad: (i32, i32),
}

impl Item {
    fn box_w(&self) -> i32 {
        self.text_w + 2 * self.pad.0
    }
    fn box_h(&self) -> i32 {
        self.text_h + 2 * self.pad.1
    }
    fn box_ascent(&self) -> f32 {
        self.ascent + self.pad.1 as f32
    }
    fn box_descent(&self) -> f32 {
        (self.text_h as f32 - self.ascent) + self.pad.1 as f32
    }
}

/// CJK 及东亚全角字符：行内任意处可断行。
fn is_cjk(c: char) -> bool {
    matches!(c,
        '\u{2E80}'..='\u{303F}'   // 部首扩展 + CJK 符号标点
        | '\u{3040}'..='\u{30FF}' // 平/片假名
        | '\u{3400}'..='\u{4DBF}'
        | '\u{4E00}'..='\u{9FFF}'
        | '\u{AC00}'..='\u{D7AF}' // 谚文音节
        | '\u{F900}'..='\u{FAFF}'
        | '\u{FF00}'..='\u{FFEF}' // 全角形式
        | '\u{20000}'..='\u{2FA1F}')
}

/// 碎片种类（切分结果）。
enum TokKind {
    Word,
    Space,
    Newline,
}

/// 把 span 文本切成不可再分碎片：Latin 词 / 空白串 / 单个 CJK 字 / 强制换行。
fn tokenize(s: &str) -> Vec<(TokKind, &str)> {
    fn flush<'a>(out: &mut Vec<(TokKind, &'a str)>, s: &'a str, from: usize, to: usize, sp: bool) {
        if to > from {
            out.push((
                if sp { TokKind::Space } else { TokKind::Word },
                &s[from..to],
            ));
        }
    }
    let mut out = Vec::new();
    let mut start = 0usize;
    let mut word = false; // 当前是否在累积 Latin 词
    let mut space = false; // 当前是否在累积空白串
    for (i, c) in s.char_indices() {
        if c == '\n' {
            flush(&mut out, s, start, i, space);
            out.push((TokKind::Newline, ""));
            start = i + c.len_utf8();
            word = false;
            space = false;
        } else if c.is_whitespace() {
            if !space {
                flush(&mut out, s, start, i, false);
                start = i;
            }
            word = false;
            space = true;
        } else if is_cjk(c) {
            flush(&mut out, s, start, i, space);
            let end = i + c.len_utf8();
            out.push((TokKind::Word, &s[i..end]));
            start = end;
            word = false;
            space = false;
        } else {
            if !word {
                flush(&mut out, s, start, i, space);
                start = i;
            }
            word = true;
            space = false;
        }
    }
    if start < s.len() {
        out.push((
            if space { TokKind::Space } else { TokKind::Word },
            &s[start..],
        ));
    }
    out
}

/// 布局器：一次 `layout_doc` 的可变状态。
struct Walker<'a> {
    m: &'a mut dyn Measurer,
    th: &'a Theme,
    wrap_w: Option<i32>,
    frags: Vec<Frag>,
    headers: Vec<(Rect, Signal<bool>)>,
    dividers: Vec<(i32, i32)>,
    y: i32,
    natural_w: i32,
    /// 是否已排过块（控制段前间距）。
    any_block: bool,
}

impl Walker<'_> {
    /// 解析 span → FragStyle（命名样式为基底、内联覆盖，再回退控件 Style）。
    fn resolve(
        &self,
        span: &RichSpan,
        styles: &HashMap<String, SpanStyle>,
        base: &Style,
    ) -> FragStyle {
        let named = span
            .named
            .as_ref()
            .and_then(|n| styles.get(n))
            .cloned()
            .unwrap_or_default();
        let s = span.style.over(&named);
        FragStyle {
            size: s.size.unwrap_or(base.font_size),
            weight: s.weight.unwrap_or(base.font_weight),
            family: s.family.or_else(|| base.font_family.clone()),
            line_height: base.line_height,
            fg: s.fg,
            bg: s.bg,
            underline: s.underline,
            strike: s.strike,
            chip: s.chip,
        }
    }

    /// 测量一个碎片为待排项。
    fn item(&mut self, text: &str, style: &FragStyle, chevron: bool, space: bool) -> Item {
        let ts = style.ts();
        let sz = self.m.size(text, &ts);
        let lm = self.m.metrics(text, &ts);
        let pad = if style.chip {
            (
                (style.size * 0.45).round() as i32,
                (style.size * 0.15).round().max(1.0) as i32,
            )
        } else {
            (0, 0)
        };
        Item {
            text: text.to_string(),
            style: style.clone(),
            chevron,
            space,
            text_w: sz.w,
            text_h: sz.h,
            ascent: lm.ascent,
            pad,
        }
    }

    /// 落定一行：基线对齐，产出 Frag，推进 y。
    fn flush_line(&mut self, line: &mut Vec<Item>, x0: i32) {
        // 行尾空白不参与行宽（但仍产出前进过的 x——已在装行时计入，无需回退视觉）。
        while line.last().map(|it| it.space).unwrap_or(false) {
            line.pop();
        }
        if line.is_empty() {
            return;
        }
        let asc = line.iter().map(Item::box_ascent).fold(0.0f32, f32::max);
        let desc = line.iter().map(Item::box_descent).fold(0.0f32, f32::max);
        let mut x = x0;
        for it in line.drain(..) {
            let top = self.y + (asc - it.box_ascent()).round() as i32;
            let rect = Rect::new(x, top, it.box_w(), it.box_h());
            x += it.box_w();
            // 纯空白且无视觉的碎片只推进 x，不产出。
            let visual =
                it.style.bg.is_some() || it.style.chip || it.style.underline || it.style.strike;
            if it.space && !visual {
                continue;
            }
            let text_rect = Rect::new(rect.x + it.pad.0, rect.y + it.pad.1, it.text_w, it.text_h);
            self.frags.push(Frag {
                text: it.text,
                rect,
                text_rect,
                ascent: it.ascent,
                style: it.style,
                chevron: it.chevron,
            });
        }
        self.natural_w = self.natural_w.max(x);
        self.y += (asc + desc).ceil() as i32;
    }

    /// 排一个段落（含 Section 头部复用：`extra` 为前置附加项，如折叠箭头）。
    fn para(
        &mut self,
        p: &Para,
        styles: &HashMap<String, SpanStyle>,
        base: &Style,
        indent: i32,
        extra: Option<Item>,
    ) {
        let x0 = indent + p.indent;
        let mut line: Vec<Item> = Vec::new();
        let mut x = x0;
        if let Some(it) = extra {
            x += it.box_w();
            line.push(it);
        }
        let spans = p.spans.clone();
        for span in &spans {
            let fs = self.resolve(span, styles, base);
            if fs.chip {
                // 胶囊整体不拆分。
                let it = self.item(&span.text, &fs, false, false);
                self.place(&mut line, &mut x, x0, it);
                continue;
            }
            for (kind, tok) in tokenize(&span.text) {
                match kind {
                    TokKind::Newline => {
                        self.flush_line(&mut line, x0);
                        // 连续空行：flush 空行无高度，显式补一行高。
                        if line.is_empty() && x == x0 {
                            let lh = fs.ts().line_height_px().unwrap_or(fs.size).ceil() as i32;
                            self.y += lh;
                        }
                        x = x0;
                    }
                    TokKind::Space => {
                        // 行首空白丢弃；空白不触发换行。
                        if line.is_empty() {
                            continue;
                        }
                        let it = self.item(tok, &fs, false, true);
                        x += it.box_w();
                        line.push(it);
                    }
                    TokKind::Word => {
                        let it = self.item(tok, &fs, false, false);
                        self.place(&mut line, &mut x, x0, it);
                    }
                }
            }
        }
        self.flush_line(&mut line, x0);
    }

    /// 装行：放不下且行非空 → 先落定当前行再放（贪心断行）。
    fn place(&mut self, line: &mut Vec<Item>, x: &mut i32, x0: i32, it: Item) {
        if let Some(w) = self.wrap_w {
            if !line.is_empty() && *x + it.box_w() > w {
                self.flush_line(line, x0);
                *x = x0;
            }
        }
        *x += it.box_w();
        line.push(it);
    }

    /// 排块序列（Section 递归）。
    fn blocks(
        &mut self,
        blocks: &[RichBlock],
        styles: &HashMap<String, SpanStyle>,
        base: &Style,
        indent: i32,
    ) {
        let spacing = self.th.rich.para_spacing();
        for b in blocks {
            match b {
                RichBlock::Para(p) => {
                    if self.any_block {
                        self.y += spacing;
                    }
                    self.any_block = true;
                    self.para(p, styles, base, indent, None);
                }
                RichBlock::Divider => {
                    // 分隔线自带上下留白，不叠加段前间距。
                    self.y += spacing;
                    self.dividers.push((indent, self.y));
                    self.y += 1 + spacing;
                    self.any_block = true;
                }
                RichBlock::Section(sec) => {
                    if self.any_block {
                        self.y += spacing;
                    }
                    self.any_block = true;
                    let collapsed = sec.collapsed.get();
                    // 头部 = 折叠箭头 + 头部段落；整行区域记为命中区。
                    let y0 = self.y;
                    let glyph = if collapsed { "▸ " } else { "▾ " };
                    let mut fs = self.resolve(
                        &RichSpan {
                            text: String::new(),
                            named: None,
                            style: SpanStyle::default(),
                        },
                        styles,
                        base,
                    );
                    fs.fg = None;
                    let chev = self.item(glyph, &fs, true, false);
                    self.para(&sec.header, styles, base, indent, Some(chev));
                    // 命中区宽度：宽度受限时撑满可用宽（好点）；Wrap 宽在收尾统一补齐。
                    let w = self.wrap_w.map(|w| w - indent).unwrap_or(0);
                    self.headers
                        .push((Rect::new(indent, y0, w, self.y - y0), sec.collapsed));
                    if !collapsed {
                        self.blocks(
                            &sec.children,
                            styles,
                            base,
                            indent + self.th.rich.section_indent(),
                        );
                    }
                }
            }
        }
    }
}

/// 全文布局。`wrap_w` 为可用宽度（None = 不限宽，逐段单行）。
fn layout_doc(
    doc: &RichDoc,
    key: LayoutKey,
    base: &Style,
    m: &mut dyn Measurer,
    th: &Theme,
) -> RichLayout {
    let mut w = Walker {
        m,
        th,
        wrap_w: key.wrap_w,
        frags: Vec::new(),
        headers: Vec::new(),
        dividers: Vec::new(),
        y: 0,
        natural_w: 0,
        any_block: false,
    };
    w.blocks(&doc.blocks, &doc.styles, base, 0);
    let natural_w = w.natural_w;
    let mut headers = w.headers;
    // Wrap 宽（无约束）时头部命中区宽度补齐到自然宽。
    for (r, _) in headers.iter_mut() {
        if r.w <= 0 {
            r.w = (natural_w - r.x).max(0);
        }
    }
    RichLayout {
        frags: w.frags,
        headers,
        dividers: w.dividers,
        size: Size::new(natural_w, w.y),
        key,
    }
}

/// 收集各 Section 折叠态快照（文档序，与布局遍历一致）。
fn collect_collapsed(blocks: &[RichBlock], out: &mut Vec<bool>) {
    for b in blocks {
        if let RichBlock::Section(sec) = b {
            let c = sec.collapsed.get();
            out.push(c);
            if !c {
                collect_collapsed(&sec.children, out);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// 控件
// ---------------------------------------------------------------------------

/// 富文本控件（见模块文档）。经 [`super::Element::rich`] 构造。
pub struct RichText {
    doc: RichDoc,
    cache: RefCell<Option<RichLayout>>,
    /// 最近一帧 paint 的 content 绝对矩形（事件坐标换算用）。
    last_content: Cell<Rect>,
    /// 悬停中的折叠头下标（headers 序）。
    hover_header: Cell<Option<usize>>,
    /// 按下时锁定的折叠头下标。
    pressed_header: Cell<Option<usize>>,
}

impl RichText {
    pub fn new(doc: RichDoc) -> Self {
        Self {
            doc,
            cache: RefCell::new(None),
            last_content: Cell::new(Rect::new(0, 0, 0, 0)),
            hover_header: Cell::new(None),
            pressed_header: Cell::new(None),
        }
    }

    fn layout_key(&self, wrap_w: Option<i32>, style: &Style, th: &Theme) -> LayoutKey {
        let mut collapsed = Vec::new();
        collect_collapsed(&self.doc.blocks, &mut collapsed);
        LayoutKey {
            wrap_w,
            family: style.font_family.clone(),
            size_bits: style.font_size.to_bits(),
            weight: style.font_weight,
            line_height_bits: style.line_height.map(f32::to_bits),
            collapsed,
            spacing: (th.rich.para_spacing(), th.rich.section_indent()),
        }
    }

    /// 确保缓存布局与 (宽度, 字体, 折叠态, 主题间距) 匹配，不匹配则重排。
    fn ensure_layout(&self, wrap_w: Option<i32>, style: &Style, m: &mut dyn Measurer) {
        let th = crate::theme::current();
        let key = self.layout_key(wrap_w, style, &th);
        let mut cache = self.cache.borrow_mut();
        let hit = cache.as_ref().map(|l| l.key == key).unwrap_or(false);
        if !hit {
            *cache = Some(layout_doc(&self.doc, key, style, m, &th));
        }
    }

    /// 命中测试折叠头（`pos` 为绝对坐标）。
    fn header_at(&self, pos: Point) -> Option<usize> {
        let content = self.last_content.get();
        let local = Point::new(pos.x - content.x, pos.y - content.y);
        let cache = self.cache.borrow();
        let lay = cache.as_ref()?;
        lay.headers.iter().position(|(r, _)| r.contains(local))
    }
}

impl Widget for RichText {
    fn measure(&self, avail: Size, style: &Style, text: &mut dyn TextEngine) -> Size {
        // 与 Label 同约定：宽度受限时按其换行；换行准确性仅保证于显式宽度
        //（width/width_match/weight），纯 Wrap 宽下为逐段单行的自然尺寸。
        let wrap_w = (avail.w > 0).then_some(avail.w);
        let mut m = EngineMeasurer(text);
        self.ensure_layout(wrap_w, style, &mut m);
        self.cache
            .borrow()
            .as_ref()
            .map(|l| l.size)
            .unwrap_or(Size::ZERO)
    }

    fn paint(
        &self,
        _bounds: Rect,
        content: Rect,
        _focused: bool,
        enabled: bool,
        canvas: &mut dyn Canvas,
        style: &Style,
    ) {
        self.last_content.set(content);
        {
            let mut m = CanvasMeasurer(canvas);
            self.ensure_layout(Some(content.w), style, &mut m);
        }
        let th = crate::theme::current();
        let pal = &th.palette;
        let cache = self.cache.borrow();
        let Some(lay) = cache.as_ref() else { return };

        for f in &lay.frags {
            let st = &f.style;
            let rect = Rect::new(
                content.x + f.rect.x,
                content.y + f.rect.y,
                f.rect.w,
                f.rect.h,
            );
            // 背景 / 胶囊底。
            let bg = match (st.bg, st.chip) {
                (Some(rc), _) => Some(rc.resolve(pal)),
                (None, true) => Some(th.rich.chip_bg(pal)),
                (None, false) => None,
            };
            if let Some(bg) = bg {
                let radius = if st.chip { rect.h as f32 / 2.0 } else { 2.0 };
                canvas.fill_round_rect(
                    rect.x as f32,
                    rect.y as f32,
                    rect.w as f32,
                    rect.h as f32,
                    radius,
                    &Paint::fill(bg),
                );
            }
            // 前景：禁用统一置灰（与 Label 同纪律）。
            let fg = if !enabled {
                pal.text_disabled
            } else if f.chevron {
                th.rich.chevron(pal)
            } else {
                match st.fg {
                    Some(rc) => rc.resolve(pal),
                    None if st.chip => th.rich.chip_fg(pal),
                    None => super::text_fg(true, style, &th),
                }
            };
            let text_rect = Rect::new(
                content.x + f.text_rect.x,
                content.y + f.text_rect.y,
                f.text_rect.w,
                f.text_rect.h,
            );
            if !f.text.trim().is_empty() {
                canvas.draw_text(&f.text, text_rect, fg, crate::spec::Align::Start, &st.ts());
            }
            // 下划线贴基线下缘、删除线穿 x 高中部；色随前景。
            let x0 = text_rect.x as f32;
            let x1 = (text_rect.x + text_rect.w) as f32;
            if st.underline {
                let y = text_rect.y as f32
                    + f.ascent
                    + ((f.text_rect.h as f32 - f.ascent) * 0.35).max(1.0);
                canvas.draw_line(x0, y, x1, y, 1.0, &Paint::fill(fg));
            }
            if st.strike {
                let y = text_rect.y as f32 + f.ascent * 0.66;
                canvas.draw_line(x0, y, x1, y, 1.0, &Paint::fill(fg));
            }
        }
        // 分隔线延展到 content 右缘。
        let dcol = if enabled {
            th.rich.divider(pal)
        } else {
            pal.divider
        };
        for &(dx, dy) in &lay.dividers {
            let y = (content.y + dy) as f32 + 0.5;
            canvas.draw_line(
                (content.x + dx) as f32,
                y,
                (content.x + content.w) as f32,
                y,
                1.0,
                &Paint::fill(dcol),
            );
        }
    }

    fn on_event(&mut self, ctx: &mut EventCtx, ev: &Event) -> bool {
        let Event::Pointer(p) = ev else { return false };
        match p.kind {
            PointerKind::Move | PointerKind::Enter => {
                let over = self.header_at(p.pos);
                if over != self.hover_header.get() {
                    self.hover_header.set(over);
                }
                false
            }
            PointerKind::Leave => {
                self.hover_header.set(None);
                false
            }
            PointerKind::Down => {
                let Some(idx) = self.header_at(p.pos) else {
                    return false;
                };
                self.pressed_header.set(Some(idx));
                ctx.capture();
                true
            }
            PointerKind::Up => {
                let Some(idx) = self.pressed_header.take() else {
                    return false;
                };
                ctx.release_capture();
                if self.header_at(p.pos) == Some(idx) {
                    if let Some(sig) = {
                        let cache = self.cache.borrow();
                        cache
                            .as_ref()
                            .and_then(|l| l.headers.get(idx))
                            .map(|(_, s)| *s)
                    } {
                        // Signal 写入自动触发重绘；折叠态入布局键，下一帧自然重排。
                        sig.set(!sig.get());
                    }
                }
                true
            }
            _ => false,
        }
    }

    fn cursor(&self) -> CursorShape {
        if self.hover_header.get().is_some() {
            CursorShape::Hand
        } else {
            CursorShape::Arrow
        }
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::Tree;
    use crate::event::{MouseButton, PointerEvent};
    use crate::signal::signal;
    use crate::ui::Element;

    /// NullTextEngine 尺寸约定：宽 = 字符数 × size × 0.6 向上取整；高 = size；
    /// 基线 = 高 × 0.8（trait 默认近似）。默认字号 14 → 单 CJK 字 9×14。
    /// 根节点会被拉伸到窗口尺寸，故把 rich 包进 col、返回其子节点 id 供断言。
    fn build(el: Element, w: i32, h: i32) -> (Tree, crate::core::NodeId) {
        let mut tree = Tree::new();
        let root = Element::col().child(el).build(&mut tree);
        tree.root = Some(root);
        tree.layout_root(Size::new(w, h), &mut crate::text::NullTextEngine);
        let child = tree.get(root).unwrap().children[0];
        (tree, child)
    }

    fn node_h(tree: &Tree, id: crate::core::NodeId) -> i32 {
        tree.get(id).unwrap().bounds.h
    }

    #[test]
    fn cjk_wraps_at_width() {
        // 20 个 CJK 字、每字 9px：95px 宽一行放 10 字（90≤95，第 11 字 99>95）→ 2 行。
        let doc = RichDoc::new().para("汉".repeat(20));
        let (tree, root) = build(Element::rich(doc).width(95), 300, 300);
        assert_eq!(node_h(&tree, root), 28, "20 字在 95px 宽应折成 2 行 × 14px");
    }

    #[test]
    fn newline_forces_break() {
        let doc = RichDoc::new().para("a\nb");
        let (tree, root) = build(Element::rich(doc).width(200), 300, 300);
        assert_eq!(node_h(&tree, root), 28, "\\n 应强制换行为 2 行");
    }

    #[test]
    fn mixed_sizes_align_on_baseline() {
        // 14px 与 28px 同行：行高 = ceil(max asc 22.4 + max desc 5.6) = 28；
        // 小字碎片 top = round(22.4 − 11.2) = 11（基线对齐产生的下沉）。
        let doc = RichDoc::new().para(Para::new().text("a").span("b", SpanStyle::new().size(28.0)));
        let rt = RichText::new(doc);
        let style = Style::default();
        let sz = rt.measure(Size::new(500, 0), &style, &mut crate::text::NullTextEngine);
        assert_eq!(sz.h, 28, "行高应取大字号的自然行高");
        let cache = rt.cache.borrow();
        let frags = &cache.as_ref().unwrap().frags;
        assert_eq!(frags.len(), 2);
        assert_eq!(frags[0].rect.y, 11, "小字应下沉到公共基线");
        assert_eq!(frags[1].rect.y, 0, "大字决定行盒、顶对齐");
    }

    #[test]
    fn chip_adds_padding_box() {
        // chip 12px："n." 文字 15×12，pad = (5,2) → 盒 25×16。
        let doc = RichDoc::new().para(Para::new().span("n.", SpanStyle::new().size(12.0).chip()));
        let rt = RichText::new(doc);
        let style = Style::default();
        rt.measure(Size::new(500, 0), &style, &mut crate::text::NullTextEngine);
        let cache = rt.cache.borrow();
        let f = &cache.as_ref().unwrap().frags[0];
        assert_eq!((f.rect.w, f.rect.h), (25, 16), "chip 盒应含内边距");
        assert_eq!((f.text_rect.w, f.text_rect.h), (15, 12), "文字矩形内缩 pad");
    }

    #[test]
    fn section_collapse_shrinks_and_click_toggles() {
        let collapsed = signal(false);
        let doc = RichDoc::new()
            .para("正文")
            .section("例句", collapsed, |d| d.para("第一句"));
        // 展开：正文 14 + 间距 6 + 头 14 + 间距 6 + 子段 14 = 54；收起：34。
        let (mut tree, root) = build(Element::rich(doc).width(200), 300, 300);
        assert_eq!(node_h(&tree, root), 54, "展开高度");

        // 点击折叠头（y ∈ [20,34)）。
        let (mut hover, mut cap) = (None, None);
        let at = crate::geometry::Point::new(10, 25);
        tree.dispatch_pointer(
            PointerEvent::single(PointerKind::Down, at, MouseButton::Left),
            &mut hover,
            &mut cap,
        );
        tree.dispatch_pointer(
            PointerEvent::single(PointerKind::Up, at, MouseButton::Left),
            &mut hover,
            &mut cap,
        );
        assert!(collapsed.get(), "点击头部应翻转折叠信号");

        tree.layout_root(Size::new(300, 300), &mut crate::text::NullTextEngine);
        assert_eq!(node_h(&tree, root), 34, "收起后高度只剩正文 + 头");
    }

    #[test]
    fn collapsed_section_children_produce_no_frags() {
        let collapsed = signal(true);
        let doc = RichDoc::new().section("头", collapsed, |d| d.para("隐藏内容"));
        let rt = RichText::new(doc);
        let style = Style::default();
        rt.measure(Size::new(300, 0), &style, &mut crate::text::NullTextEngine);
        let cache = rt.cache.borrow();
        let frags = &cache.as_ref().unwrap().frags;
        // 只有箭头 + 头文字两个碎片；子内容不产出。
        assert_eq!(frags.len(), 2, "折叠区子内容不应产出碎片");
    }

    #[test]
    fn named_style_resolves_and_inline_overrides() {
        let doc = RichDoc::new()
            .style("big", SpanStyle::new().size(20.0).bold())
            .para(Para::new().styled_span("big", "x", SpanStyle::new().size(30.0)));
        let rt = RichText::new(doc);
        let style = Style::default();
        rt.measure(Size::new(300, 0), &style, &mut crate::text::NullTextEngine);
        let cache = rt.cache.borrow();
        let f = &cache.as_ref().unwrap().frags[0];
        assert_eq!(f.style.size, 30.0, "内联字号应覆盖命名样式");
        assert_eq!(f.style.weight, 700, "未覆盖字段继承命名样式");
    }

    #[test]
    fn spaces_do_not_trigger_wrap_and_drop_at_line_start() {
        // "aa bb"（词 17px+空 9? — 空格 1 字 → ceil(0.6*14)=9；aa=17,bb=17）宽 40：
        // aa(17)+空(9)=26，bb 需 26+17=43>40 → 换行，bb 行首无空格。
        let doc = RichDoc::new().para("aa bb");
        let rt = RichText::new(doc);
        let style = Style::default();
        let sz = rt.measure(Size::new(40, 0), &style, &mut crate::text::NullTextEngine);
        assert_eq!(sz.h, 28, "应折成两行");
        let cache = rt.cache.borrow();
        let frags = &cache.as_ref().unwrap().frags;
        assert_eq!(frags.len(), 2, "空白碎片不应产出");
        assert_eq!(frags[1].rect.x, 0, "第二行行首不应残留空格缩进");
    }
}

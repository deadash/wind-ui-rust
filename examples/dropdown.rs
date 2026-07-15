//! 下拉选择 Dropdown 示例。
//!
//! 运行：cargo run --release --example dropdown
//! 闭合截屏：cargo run --example dropdown -- --screenshot artifacts/dropdown.png
//! 展开截屏（纯文本）：cargo run --example dropdown -- --screenshot artifacts/dropdown_open.png --click 120 96
//! 展开截屏（富内容：副标题 + 徽章 + 可点击尾随图标）：
//!   cargo run --example dropdown -- --screenshot artifacts/dropdown_open_rich.png --click 140 245

use windui::prelude::*;

const BG: u32 = 0xEEF1F5;

fn label(t: &str) -> Element {
    Element::label(t)
        .font_size(13.0)
        .fg(Color::hex(0x636E72))
        .height(20)
        .width_match()
}

fn main() {
    let theme = signal(1usize);
    let quality = signal(0usize);
    let plan = signal(0usize);

    let plan_items = vec![
        DropdownItem::new("免费版").badge("当前", Intent::Neutral),
        DropdownItem::new("专业版")
            .subtitle("解锁全部导出格式")
            .badge("推荐", Intent::Primary),
        DropdownItem::new("团队版")
            .subtitle("多人协作 + 权限管理")
            .badge("New", Intent::Danger)
            .trailing_icon("🗑", || println!("点击了团队版的尾随图标（未选中该项）")),
    ];

    let ui = Element::col()
        .fill()
        .bg(Color::hex(BG))
        .padding(20)
        .spacing(10)
        .child(
            Element::label("下拉选择")
                .font_size(22.0)
                .fg(Color::hex(0x1A1A2E))
                .height(30)
                .width_match(),
        )
        .child(label("主题"))
        .child(Element::dropdown(vec!["跟随系统", "浅色", "深色"], theme).width(220))
        .child(label("渲染质量"))
        .child(Element::dropdown(vec!["低", "中", "高", "极致"], quality).width(220))
        .child(label("方案（富内容：副标题 + 徽章 + 可点击尾随图标）"))
        .child(Element::dropdown_items(plan_items, plan).width(260));

    App::new("windui — 下拉选择", 320, 360)
        .bg(Color::hex(BG))
        .screenshot_from_args()
        .content(ui)
        .run();
}

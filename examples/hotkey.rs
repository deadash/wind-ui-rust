//! 全局热键 + 启动即隐藏 + 托盘：常驻后台小工具的完整骨架。
//!
//! 运行：`cargo run --release --example hotkey`
//!
//! - 启动**不显示窗口**，只在托盘出现一个图标。
//! - 按 **Ctrl+Alt+D**（任何程序里都行，本窗口无需焦点）唤起窗口并置前。
//! - 按 **Ctrl+Alt+H** 隐藏窗口。
//! - 窗口内点「隐藏到托盘」按钮同样隐藏（走 `EventCtx::hide_window`）。
//! - 托盘右键 → 退出。
//!
//! 热键消息由系统投递到本窗口队列，空闲时仍阻塞在 `GetMessageW`——**零 CPU 占用**。

use windui::prelude::*;

/// 生成 size×size 纯色 RGBA8（演示图标，免捆绑资源）。
fn solid(size: u32, hex: u32) -> Vec<u8> {
    let (r, g, b) = (
        ((hex >> 16) & 0xff) as u8,
        ((hex >> 8) & 0xff) as u8,
        (hex & 0xff) as u8,
    );
    [r, g, b, 255].repeat((size * size) as usize)
}

fn main() {
    let hits = signal(0u32);
    let hits_text = signal(String::from("热键唤起次数：0"));
    let status = signal(String::from("按 Ctrl+Alt+D 可随时唤起本窗口"));

    let tray = Tray::new()
        .tooltip("windui 全局热键示例")
        .icon_rgba(16, 16, &solid(16, 0x6C5CE7))
        .on_left_click(|ctx| ctx.show_window())
        .menu(vec![
            TrayMenuItem::item("显示窗口", |ctx| ctx.show_window()),
            TrayMenuItem::separator(),
            TrayMenuItem::item("退出", |ctx| ctx.quit()),
        ]);

    let ui = Element::col()
        .fill()
        .bg(Color::hex(0xFFFFFF))
        .padding(24)
        .spacing(12)
        .child(
            Element::label("全局热键")
                .font_size(22.0)
                .fg(Color::hex(0x2D3436))
                .height(30)
                .width_match(),
        )
        .child(Element::label_rc(status).height(22).width_match())
        .child(Element::label_rc(hits_text).height(22).width_match())
        .child(Element::divider())
        .child(
            Element::label("Ctrl+Alt+D 唤起 · Ctrl+Alt+H 隐藏 · 关窗即退出")
                .fg(Color::hex(0x636E72))
                .height(20)
                .width_match(),
        )
        // 控件回调里请求隐藏：走 EventCtx::hide_window → WindowOp::Hide。
        .child(Element::button("隐藏到托盘").on_click(|ctx| ctx.hide_window()));

    App::new("全局热键", 380, 240)
        .tray(tray)
        .start_hidden()
        // 回调只声明意图，拿不到窗口句柄——见 App::hotkey 文档中的借用纪律。
        .hotkey(Hotkey::new(Key::Char('D')).ctrl().alt(), move |ctx| {
            hits.set(hits.get() + 1);
            hits_text.set(format!("热键唤起次数：{}", hits.get()));
            ctx.show_window();
        })
        .hotkey(Hotkey::new(Key::Char('H')).ctrl().alt(), |ctx| {
            ctx.hide_window()
        })
        // 截屏走离屏路径，不创建窗口，故与 start_hidden 无冲突。
        .screenshot_from_args()
        .content(ui)
        .run();
}

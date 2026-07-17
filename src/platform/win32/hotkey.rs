//! 全局热键（RegisterHotKey）：应用无焦点、窗口隐藏时亦可触发。
//!
//! 热键消息 `WM_HOTKEY` 由系统投递到本窗口的消息队列——**事件驱动，不轮询**，
//! 故不破坏「空闲零 CPU」这条核心指标（AGENTS.md）。
//!
//! ## 借用纪律
//!
//! 回调拿到的 [`HotkeyCtx`] **不持有 hwnd**，只能声明 [`WindowOp`] 意图。这是刻意的：
//! 回调在 `wnd_proc` 里持有 `WindowState` 借用期间执行，此时若直接调 `ShowWindow` /
//! `SetForegroundWindow`，这些 API 会**同步**派发 `WM_SHOWWINDOW` / `WM_ACTIVATE`
//! 回 `wnd_proc`，那里再 `state_from` 一次即造成 `&mut` 别名（铁律 6）。
//!
//! 把窗口操作降级为「意图」、由调用方在借用释放后统一执行，使该约束成为**类型上的
//! 保证**而非人的记性——ctx 里根本没有句柄，危险代码写不出来。

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    RegisterHotKey, UnregisterHotKey, HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT,
    MOD_SHIFT, MOD_WIN, VK_DELETE, VK_DOWN, VK_END, VK_ESCAPE, VK_HOME, VK_LEFT, VK_RETURN,
    VK_RIGHT, VK_SPACE, VK_TAB, VK_UP,
};

use crate::event::{Hotkey, HotkeyCtx, Key, WindowOp};
use crate::platform::HotkeyBinding;

/// 热键回调。`None` 表示该槽注册失败。
type Slot = Option<Box<dyn FnMut(&mut HotkeyCtx)>>;

/// 已注册的热键集合。窗口销毁时 `drop` 自动注销。
pub(crate) struct HotkeyState {
    hwnd: HWND,
    /// 索引即 `RegisterHotKey` 的 id，亦即 `WM_HOTKEY` 的 wParam。
    /// 注册失败的槽位置 `None`，以保持 id 与索引对齐——用紧凑数组会让 id 错位，
    /// 令 WM_HOTKEY 触发到错误的回调。
    bindings: Vec<Slot>,
}

impl HotkeyState {
    /// 注册全部热键。
    ///
    /// 单个热键注册失败**不影响其余热键**，也不阻止窗口创建：热键是全局独占资源，
    /// 组合被别的程序占用是常态而非异常，让整个应用起不来是不可接受的。失败者静默
    /// 忽略——当前没有向应用回报失败的 API，需要时再加。
    pub(crate) fn register(hwnd: HWND, bindings: Vec<HotkeyBinding>) -> Self {
        let mut slots = Vec::with_capacity(bindings.len());

        for (id, b) in bindings.into_iter().enumerate() {
            // 无法映射为虚拟键（非 ASCII 字符、越界 VK 等）→ 置空槽。
            let registered = vk_of(b.hotkey.key).is_some_and(|vk| {
                // MOD_NOREPEAT：按住不放时只触发一次。缺了它，长按热键会以键盘重复率
                // 刷屏般触发回调。
                let mods = mods_of(b.hotkey) | MOD_NOREPEAT;
                unsafe { RegisterHotKey(Some(hwnd), id as i32, mods, vk) }.is_ok()
            });
            // 失败槽置 None 而非跳过：id 即索引，紧凑数组会让后续 id 错位，
            // 令 WM_HOTKEY 触发到错误的回调。
            slots.push(registered.then_some(b.callback));
        }

        Self {
            hwnd,
            bindings: slots,
        }
    }

    /// 派发一条 `WM_HOTKEY`，返回回调声明的窗口操作意图。
    ///
    /// **调用方必须在释放 `WindowState` 借用之后**再执行返回的 [`WindowOp`]——
    /// 见本模块头部的借用纪律。
    #[must_use]
    pub(crate) fn dispatch(&mut self, id: usize) -> Option<WindowOp> {
        let cb = self.bindings.get_mut(id)?.as_mut()?;
        let mut ctx = HotkeyCtx::default();
        cb(&mut ctx);
        ctx.take_op()
    }
}

impl Drop for HotkeyState {
    fn drop(&mut self) {
        for (id, slot) in self.bindings.iter().enumerate() {
            // 只注销注册成功的：对未注册的 id 调 UnregisterHotKey 会失败（无害但无意义）。
            if slot.is_some() {
                unsafe {
                    let _ = UnregisterHotKey(Some(self.hwnd), id as i32);
                }
            }
        }
    }
}

/// 修饰键 → Win32 标志。
fn mods_of(hk: Hotkey) -> HOT_KEY_MODIFIERS {
    let mut m = HOT_KEY_MODIFIERS(0);
    if hk.mods.ctrl {
        m |= MOD_CONTROL;
    }
    if hk.mods.alt {
        m |= MOD_ALT;
    }
    if hk.mods.shift {
        m |= MOD_SHIFT;
    }
    if hk.mods.meta {
        m |= MOD_WIN;
    }
    m
}

/// `Key` → 虚拟键码。无法映射者返回 `None`。
///
/// 字母与数字的 VK 码等于其**大写 ASCII** 值（`VK_A == 0x41 == b'A'`），故直接换算，
/// 不必逐个枚举。非 ASCII 字符（如 `Key::Char('中')`）没有稳定的 VK 映射——它依赖
/// 当前键盘布局，作全局热键无意义，故拒绝。
///
/// `Key::Other(vk)` **直接放行**：本仓库已把它定义为跨平台对齐的虚拟键码
/// （win32 由 `to_key` 产出原始 VK；macOS 亦按 win32 VK 码对齐，见 `macos/window.rs`）。
/// 它是 F1–F12、PageUp/PageDown、Insert 等键位的**唯一表达途径**——`Key` 枚举没有
/// 这些变体，堵死 `Other` 等于让 `Ctrl+Alt+F1` 这类常见全局热键无法注册。
fn vk_of(key: Key) -> Option<u32> {
    let vk = match key {
        Key::Char(c) if c.is_ascii_alphanumeric() => c.to_ascii_uppercase() as u32,
        Key::Char(_) => return None,
        Key::Tab => VK_TAB.0 as u32,
        Key::Enter => VK_RETURN.0 as u32,
        Key::Escape => VK_ESCAPE.0 as u32,
        Key::Space => VK_SPACE.0 as u32,
        Key::Left => VK_LEFT.0 as u32,
        Key::Right => VK_RIGHT.0 as u32,
        Key::Up => VK_UP.0 as u32,
        Key::Down => VK_DOWN.0 as u32,
        Key::Home => VK_HOME.0 as u32,
        Key::End => VK_END.0 as u32,
        Key::Delete => VK_DELETE.0 as u32,
        // 虚拟键码是 8 位的；越界值必是调用方搞错了，与其注册出个诡异热键不如拒绝。
        Key::Other(vk) if vk <= 0xFF => vk,
        Key::Other(_) => return None,
        // Backspace 作全局热键无实际用途。
        Key::Backspace => return None,
    };
    Some(vk)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 字母键映射为大写ascii的虚拟键码() {
        // VK_A == 0x41 == b'A'。小写输入也须映射到同一个码。
        assert_eq!(vk_of(Key::Char('d')), Some(0x44));
        assert_eq!(vk_of(Key::Char('D')), Some(0x44));
        assert_eq!(vk_of(Key::Char('0')), Some(0x30));
    }

    #[test]
    fn 非ascii字符不可作热键() {
        // 汉字没有稳定的 VK 映射——取决于键盘布局，作全局热键无意义。
        assert_eq!(vk_of(Key::Char('中')), None);
        assert_eq!(vk_of(Key::Char('é')), None);
    }

    #[test]
    fn 具名键映射到对应虚拟键码() {
        assert_eq!(vk_of(Key::Escape), Some(VK_ESCAPE.0 as u32));
        assert_eq!(vk_of(Key::Space), Some(VK_SPACE.0 as u32));
    }

    #[test]
    fn 无意义的键被拒绝() {
        assert_eq!(vk_of(Key::Backspace), None);
    }

    #[test]
    fn other放行为虚拟键码() {
        // Key::Other 在本仓库即跨平台对齐的 VK 码，且是 F 键等键位的唯一表达途径。
        // VK_F1 == 0x70。堵死它等于让 Ctrl+Alt+F1 无法注册。
        assert_eq!(vk_of(Key::Other(0x70)), Some(0x70));
        assert_eq!(vk_of(Key::Other(0x41)), Some(0x41));
    }

    #[test]
    fn 越界的虚拟键码被拒绝() {
        // VK 是 8 位的；越界必是调用方搞错了。
        assert_eq!(vk_of(Key::Other(0x100)), None);
        assert_eq!(vk_of(Key::Other(u32::MAX)), None);
    }

    #[test]
    fn 修饰键组合为标志位() {
        let hk = Hotkey::new(Key::Char('D')).ctrl().alt();
        let m = mods_of(hk);
        assert_eq!(m & MOD_CONTROL, MOD_CONTROL);
        assert_eq!(m & MOD_ALT, MOD_ALT);
        assert_eq!(m & MOD_SHIFT, HOT_KEY_MODIFIERS(0), "未声明 shift");
        assert_eq!(m & MOD_WIN, HOT_KEY_MODIFIERS(0), "未声明 meta");
    }

    #[test]
    fn 无修饰键时标志为零() {
        assert_eq!(mods_of(Hotkey::new(Key::Escape)), HOT_KEY_MODIFIERS(0));
    }
}

# Changelog

本项目遵循 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)，
版本号遵循 [语义化版本](https://semver.org/lang/zh-CN/)。

## [Unreleased]

### Added
- **`Widget::tooltip()` 动态悬停提示**：控件可按当前指针位置自报提示文本，优先于节点上
  `.tooltip(..)` 设的静态文本，返回 `None` 则回退到静态文本（没有则不弹）。
  给自绘图表类控件用——整张图是一个节点，提示内容取决于指针落在哪个数据点上
  （日历热力图的哪一格、柱状图的哪一根），静态文本表达不了。控件在 `on_event` 里记下
  命中项、在 `tooltip()` 里据此返回文案即可，浮层的延时/跟随/边缘翻转仍由宿主统一处理。
  默认实现返回 `None`，既有控件不受影响。

## [0.9.0] - 2026-07-23

本版本新增 RichText 富文本控件与全局热键管线，并把文字属性收进 `TextStyle`——后者改动了
`TextEngine` / `Canvas` 两个 trait 的签名，自定义渲染后端需要跟随调整（见 Changed 的破坏性条目）。

### Added
- **`RichText` 富文本控件**（`Element::rich` / `rich_rc`）：段落 + 碎片（span）模型，配套能力如下。
  - **排版**：CJK 避头尾（闭合标点不落行首、开括类不孤悬行尾）、`Para::hanging` 悬挂缩进
    （编号义项续行对齐释义首字）、`Para::spacing_before` 按段覆盖段距。
  - **span 点击**：`Para::span_id` / `styled_id` 标注纯数据 id，回调经 `Element::on_span_click`
    挂在控件层——`RichDoc` 保持 `Clone` / 可比较 / 可缓存。悬停手型 + 同 id 跨行碎片一起提亮。
  - **划选复制**：碎片级选区（CJK 逐字、Latin 整词吸附、chip 整体）、选区高亮、`Ctrl+C` 复制选区、
    `Ctrl+Shift+C` 强制全文、`Ctrl+A` 全选，右键菜单按选区态给「复制 / 复制全部 / 全选」。
    跨块补换行、块内软换行按 CJK/Latin 边界补空格。
  - **双击选词 / 三击选段**：双击对 CJK 吞并同块内连续汉字碎片（至标点/空白/chip 边界止），
    三击选中命中碎片所在段落全部碎片（含软换行续行、不跨段），对齐浏览器习惯。
  - **折叠 Section**：可 `Tab` 聚焦，`↑↓` 在折叠头间移动、`Enter`/`Space` 翻转；展开/收起为
    卷帘高度动画（收拢中按目标状态完整排版，对外只占补间高度）。
  - **行数截断**：`Para::clamp(max_lines, expanded)` 未展开只排 N 行，行尾缀可点击的「… 展开」标记
    （不计入复制文本）。
  - **动态文档**：`Element::rich_rc(Signal<RichDoc>)` 整篇换文档，同步失效布局缓存与选区、复位悬停
    与键盘焦点下标。
  - `RichDoc::plain_text`（含 chip 与折叠区文字）与内建右键「复制全部」菜单，`Element::copy_menu(false)` 可关闭。
- **全局热键**：`App::hotkey` 注册全局热键、`App::start_hidden` 启动不显示窗口、
  `EventCtx::show_window` / `hide_window`，`WindowOp` 增 `Show` / `Hide`。回调只拿意图不拿句柄
  （`HotkeyCtx` 仅持 `Option<WindowOp>`），窗口操作在平台层释放借用后执行。注册失败不阻止启动。
  Windows 走 `RegisterHotKey` + `WM_HOTKEY`；macOS 待补。
- **热键运行期改绑**：`App::hotkey_rc` 返回 `HotkeyHandle`，`rebind(hotkey)` / `set_enabled(bool)`
  运行期即时生效（此前仅启动期一次性注册，改热键须重启）。改绑失败回滚重注册旧组合，
  `set_enabled(false)` 注销把组合归还系统。
- **主题运行期动态更新**：`ThemeHandle::update(|t| ...)` 局部改主题（换强调色/调字号一行完成，
  下一帧全树跟随）；新增 `Brush::RoleAlpha(Role, alpha)`、`Element::bg_role_alpha` 与
  `Role::InputBg` / `InputBorder`，把构建期取色改为角色延迟解析——徽章/chip/标签输入/对话框面板/
  表格编辑格换主题后自动跟随，不再停在旧主题色。
- **关闭即隐藏**：`App::hide_on_close()` 把 `ESC` 与标题栏关闭按钮转为隐藏窗口，退出留给托盘菜单
  （常驻托盘类应用的常见期望）。拦截器优先级高于它——`close_handler` 返回 `false` 时窗口既不关也不隐。
- **文字排版三项**：`Element::line_height(倍数)`（取倍数使行距随字号与 DPI 缩放）、
  `Element::max_width(px)`（测量前收窄可用宽，内容据此换行而非事后裁切）、
  `Element::border_edges(Edges)` 单边边框（页签下划线、分区底线不必再用 1px 色块拼）。
- **字体族**：`Element::font_family(name)` 指定字体族名（Windows/macOS 均生效）。字体未安装时静默回退系统默认，不报错也不 panic。
- **节点级焦点覆盖**：`Element::focusable(bool)` 控制 `Tab` 遍历是否纳入该节点（不改命中/拖动/`request_focus` 语义）。
- **胶囊式标签条**：`TabStyle::Pill` 与 `Element::tabs_pill`——accent 实底胶囊 + 白字滑动。
- **下拉项富信息**：`MenuItem` 新增 `subtitle` / `badge` / `trailing_icon`，展开态支持两行项与徽章胶囊，
  尾随图标点击独立于主项 action；收起态同步显示选中项徽章。新增 `DropdownItem` 与
  `Element::dropdown_items`，纯文本 `Vec<String>` 旧用法零改动。
- **表格整行双击激活**：`Element::on_row_activate`（释放 `Up` 时触发）。
- **无边框窗口圆角**：`frameless()` 窗口在 Win11 上显式声明 `DWMWA_WINDOW_CORNER_PREFERENCE`，与系统其余窗口一致；Win10 上 DWM 不识别该属性、返回错误码并被忽略，无需版本判断。macOS 由 AppKit 天然保持圆角。

### Changed
- **（破坏性）文字属性收进 `TextStyle`**：`TextEngine::measure` / `line_metrics` 与
  `Canvas::measure_text` / `draw_text` 改为接收 `&TextStyle`，字族/字号/字重/行高一并传递；
  原先的线程局部字重注入（`text::set_weight` / `current_weight`）随之删除——那让字重成了隐式全局
  状态，漏复位就会让后续无关文字跟着变粗。自定义 `TextEngine` / `Canvas` 实现需按新签名调整；
  控件调用方改为 `&TextStyle::of(style)`，比原先的散开参数更短。
- **（破坏性）`TrayCtx` 改意图队列**：不再持有 `hwnd`/`uid`，四个方法只累积 `TrayAction`，由平台层在
  释放借用后执行；macOS `TrayCtx` 同步改 `&mut self`，使两平台签名一致。
- **标签条重做为下划线式**：`TabButton` 逐节点 → 单个自绘 `TabBar`，选中项为整格宽指示条 + 贯穿基线，
  切换时横向滑动；去掉选中焦点框与悬停淡底，选中态加粗且按选中字重恒定测量以免布局抖动。
  整条为一个焦点节点、内部 `Left`/`Right` 移动，符合 tablist roving tabindex 约定。
- **chip 前景对比度**：默认前景按 WCAG AA 自适应——从 accent 向正文色插值直到对实际底色 ≥4.5:1
  （「同色淡底 + 同色前景」实测仅约 3:1）。
- **事件路径时间源**：新增 `EventCtx::now_ms` 作为事件回调中的推荐时间源。

### Fixed
- **托盘回调重入 UB**：`WM_TRAYICON` 在持有 `&mut WindowState` 期间跑用户回调，而回调经 `TrayCtx`
  直接调 `ShowWindow`/`DestroyWindow`、右键还调模态的 `TrackPopupMenu`，重入 `wnd_proc` 后再取一次
  `&mut WindowState` 即别名 UB；其中 `quit()` 的 `DestroyWindow` 会同步 drop 掉正在执行的闭包本身，
  属 use-after-free。改为意图队列后消除。顺带修正点托盘图标唤不起最小化窗口（`SW_SHOW` → `WindowOp::Show`）。
- **帧时钟在事件路径冻结**：`clock_ms()` 此前只在 render 前刷新，空闲不出帧期间停在上一帧，
  两次交互之间的静默期被整段计入时长判定（长按、双击、拖动速度均受影响）。`on_pointer`/`on_key`
  入口也同步帧时钟。
- **步进器点击即进快速加**：长按起点改由按下后首帧 paint 用刚刷新的帧时钟锚定，不再在事件路径读冻结时钟。
- **清屏色不随主题热切换**：未经 `App::bg` 显式固定时，`UiHost` 每帧跟随 `palette.bg`——修「切暗色主题后
  清屏/局部重绘仍是亮色底」。`theme()` 不再覆盖显式 `bg`（`.bg(c).theme(t)` 与反序同义）。
- **下拉徽章灰字灰底**：Neutral 意图徽章前景改用 `text_muted`。
- **最小化/最大化动画期左上角内容被拉伸**：flip-model 交换链下 `ResizeBuffers` 到重绘落地之间存在真空期，
  DWM 会采样旧尺寸缓冲并按 `DXGI_SCALING_STRETCH` 从左上角拉伸。非拖拽的最大化/还原改同步重绘
  （拖拽缩放中保持异步以免拖累手感）、跳过 `SIZE_MINIMIZED`、交换链 Scaling 改 `NONE`。
- **单实例转发失败被挡在门外**：首实例退出中或僵死时 `WM_COPYDATA` 同步发送会把二次实例一起挂住；
  改用 `SendMessageTimeoutW(SMTO_ABORTIFHUNG)` 探测送达失败并回退为正常启动新窗口。
- **表格多行单元格顶部对齐**：多行分支由 stack 改为 row + `cross(Center)`，同行折行撑高时单行文本格竖直居中。
- **富文本布局缓存每帧堆分配**：`ensure_layout` 命中判定改引用比较 + 零分配快路径，仅 miss 时构造 `LayoutKey`。

## [0.8.3] - 2026-07-13

### Added
- **表格单元格多行**：`Table` 单元格支持多行文本，新增 `cell_lines(n)` 配置显示行数。

### Fixed
- **表格多行单元格裁切**：多行单元格内容被错误裁切，修正行高与裁剪区计算。
- **`on_update` 相位的 toast 被丢弃**：在 `on_update` 阶段调用 `ctx.toast*` 发出的浮层不再被丢弃。
- **对话框复显开关瞬时落定**：对话框重新显示时开关状态瞬时正确落定；文本输入清除残留选区。
- **无边框窗口标题栏区域 toast 失效**：无边框窗口标题栏区域的 toast 被命中判定为客户区，修复其上 ✕ 关闭 / 右键菜单失效。

### Changed
- **toast 面板样式**：降低面板高度、移除强调色条，右键菜单置于 toast 之上。

## [0.8.2] - 2026-07-06

### Fixed
- **连续空格中光标无法移动**：`DWRITE_TEXT_METRICS::width` 不含尾随空白宽度，导致以
  空格结尾的子串测量宽度被折叠为同一值——文本框光标索引在连续空格中正确递增，但换算出的
  视觉 x 坐标不再前进，表现为"光标卡在第一个非空格字符处"。改用
  `widthIncludingTrailingWhitespace` 字段（`src/text/dwrite.rs`、`src/platform/win32/d2d.rs`）。
- **输入法组合态期间自绘光标位置错误**：拼音等未上屏组合期间，`TextInput`/`Stepper` 自绘的
  光标条停留在组合开始前的位置不动，与系统组合浮层里跟随合成进度前进的光标同时存在，视觉上
  像卡住。新增 `Widget::set_composing`，由平台层在 Windows 的
  `WM_IME_STARTCOMPOSITION`/`WM_IME_ENDCOMPOSITION`、macOS 的
  `setMarkedText`/`unmarkText`/`insertText:` 时通知焦点控件，组合期间跳过自绘光标绘制，
  交由系统浮层呈现。
- **输入法组合串字体与正文不一致**：Windows 合成串 `LOGFONTW.lfFaceName` 之前留空，系统常
  回退到陈旧的宋体；现显式指定为与正文渲染同族的 `Microsoft YaHei UI`。

## [0.8.1] - 2026-07-06

### Added
- **`PickDialog` 同步方法误用检测**：`pick_file`/`pick_files`/`pick_folder`/`pick_folders`/
  `save_file` 在控件事件回调（`on_click`/`on_event`）栈内被调用时，`debug_assert!` 报错
  （release 构建零开销剔除）——把"回调里别同步开模态对话框，OS 捕获来不及释放会导致鼠标
  失灵"这条只写在文档注释里的契约，变成 debug/测试阶段能捕获到的确定性失败，而不是留到运行时
  变成偶发的鼠标卡死。内部用线程局部 `EventDispatchGuard` 标记风险窗口（`on_pointer`/`on_key`/
  `on_drop_files` 分发期间），win32/macos 两个后端均已接入；`app.rs::on_drop_files` 同时补上了
  之前遗漏的 `dialog` 请求转发（`Element::on_drop` 回调里调用 `EventCtx::request_*` 之前会被
  静默丢弃）。

## [0.8.0] - 2026-07-06

### Added
- **`DialogRequest` + `EventCtx::request_pick_file`/`request_pick_files`/`request_pick_folder`/
  `request_pick_folders`/`request_save_file`/`defer_blocking`**：原生文件对话框不再在事件回调
  栈内同步弹出——按钮点击回调里直接调用 `PickDialog::pick_file()` 等阻塞方法时，OS 鼠标捕获的
  释放要等整条事件分发调用栈返回才生效，导致对话框存续期间主窗口仍持有 `SetCapture`，与对话框
  自己的消息泵抢鼠标输入，反复开关几次后捕获状态与 OS 实际状态错位，表现为鼠标彻底失灵。
  现改为把对话框请求（`PickDialog` + 结果延续回调，或 `defer_blocking` 逃生舱包一段任意阻塞式
  原生调用序列）经 `EventCtx`/`DispatchResult` 交给宿主，在事件分发**完全返回**、OS 捕获同步
  完毕之后才真正执行。`PickDialog` 本身的同步 API 仍保留（非 UI 回调场景可用），但**不要**在
  `on_click`/`on_event` 回调里直接调用。
- **表格自定义单元格渲染 `Element::cell_render`**：按 `(行下标, 列下标, 单元格文本)` 逐格询问，
  返回 `Some(Element)` 用自定义控件（徽章/彩色标签/图标等），`None` 回退默认文本。排序仍基于
  单元格文本（渲染与排序键解耦）；行下标语义同 `.actions`（客户端表格为原始行下标，服务端表格
  为页内显示下标）。适用于 `table_sortable` / `table_sortable_server` / `table_selectable`，
  可与 `.actions` 组合。fullshowcase 表格 tab 新增演示。
- **`Element::host_signal`**：信号驱动的响应式重建宿主。同 `list_signal` 的重建机制，但容器为
  普通列容器（非滚动）——子元素 `weight`/`fill` 能拿到确定高度，适合整体重建"结构随状态变化"
  的子树（如列集随类别切换的表格；滚动容器按无限高度测量会令表格正文高度崩塌）。

### Fixed
- 响应式广播（`dispatch_reactive_updates`）曾用广播快照的存活集**覆盖**注册列表，把广播期间
  动态重建子树新注册的响应式节点抹掉——`list_signal`/`host_signal` 重建出的响应式表头/正文
  永远收不到 `on_update`，表格在宿主重建后空白。现改为按批次迭代到收敛（新注册节点**同帧**
  收到回调，避免首帧空白），清理阶段基于真实列表 retain。

### Changed
- `DispatchResult` 不再 `derive(Clone)`（新增字段携带 `Box<dyn FnOnce()>`，不可 Clone；原实现
  从未实际克隆过该结构，纯类型层面的收紧）。

## [0.4.0] - 2026-06-26

### Added
- **Direct2D GPU 渲染后端（Windows，可选 opt-in）**：大窗口/多控件下软件光栅 paint-bound，新增
  Direct2D 后端把几何/渐变/裁剪/opacity/图片/阴影/文字光栅迁到 GPU。窗口级显式 opt-in
  `App::accelerated(true)`（示例 `--accelerated`），**默认仍软渲染**；与 tiny-skia 软路径并存。
  - 文字坚持走 **DirectWrite**（`DrawTextLayout`，系统字体缓存 + ClearType），与软路径字体/字重一致。
  - 阴影用 `ID2D1Shadow` GPU 高斯模糊，烘焙一次缓存成品避免每帧重模糊。
  - 自动回退软渲染（绝不 panic）：RDP 远程会话、无可用 GPU、设备创建失败、离屏截图。
  - 设备丢失检测 → 整体重建设备链 → 连续失败降级软后端；同 UI 线程多窗口共享设备链（避免 ×N 内存）。
  - 重对象（文字布局/画刷/位图/后备缓冲）全缓存复用，常驻内存从早期 190M 降到 ~70M。
- 渐变画刷（线性/径向）+ `Brush`（Solid/Gradient/Role）主题角色取色体系。
- `Theme::dark` 暗色预设 + `ThemeHandle` 运行期主题热切换（整树跟随刷新）。
- 浮层投影（box-shadow）+ 子树整体不透明度（离屏层合成）。
- 级联右键菜单（图标/分隔/快捷键/子菜单）+ `Element::on_context_menu`。
- `PickDialog`：系统原生文件/目录选择对话框。
- `Signal<T>`：`Copy` 句柄状态原语（运行时 arena 承载），全控件状态从 `Rc<Cell>`/`Rc<RefCell>` 迁入；
  `set` 自动产生局部脏区，新控件免手写 `mark_dirty`。
- 文字字重支持；半透明文字色。
- `App::min_size`：限制窗口最小客户区尺寸。
- 新增 `examples/ime.rs`（复刻中文输入法界面，暗/亮双主题）。

### Changed
- 控件状态原语统一为 `Signal<T>`，取代散落的 `Rc<Cell>`/`Rc<RefCell>`（API 基本不变，状态语义更一致）。
- 渲染接缝重构：`AppHandler::render` 改为面向 `RenderTarget`，软/GPU 两后端同形接入，软路径零回归。

### Performance
- 交互失效系统：hover/拖动/点击/打字走 ~1ms **局部重绘**（结构签名判定局部 vs 整窗），不再每次整窗重绘。
- DirectWrite 测量结果缓存，消除稳定文本每帧重复排版。
- 模糊阴影缓存（位置无关），修复阴影每帧重算导致的卡顿；新增 `WINDUI_PROF` 绘制热点计时。

### Fixed
- 窗口按钮与复选框的文字/悬停色未跟随主题。
- DPI 缩放下 win32 窗口显示异常（全窗重绘 scale 由 handler 提供）。
- 点击切换内容不刷新；标签条内边距、菜单尾随快捷键换行、分段选中反色、菜单高亮溢出等多处 UI 细节。

## [0.3.0] - 2026-06-23

### Added
- 多行 `TextInput`：滚动条、滚轮滚动、跨视口拖选。
- `Label` `max_lines` 行数限制 + Truncate 省略号（End/Start/Middle）。

### Fixed
- `ScrollWidget` 滚轮滚动到边界时冒泡给外层容器。

## [0.2.0] - 2026-06-23

### Added
- 跨线程 UI 更新：`App::channel::<Msg>(on_message) -> Sender<Msg>`（后台 `send` 事件驱动唤醒 UI、`on_message` 在 UI 线程写状态）+ `App::on_interval(dur, cb)` 定时回调。有更新才重绘、空闲零 CPU。
- 语义意图色（Intent）体系：Button / CheckBox 统一 `.intent()` / `.danger()` / `.neutral()` / `.accent(color)`；
  内置 primary/neutral/danger，`Custom(Color)` 为扩展点——单基色自动派生 hover/active + 对比自适应前景。
  Button 默认 Primary（现有代码零改动）；CheckBox 现有 `.danger()`/`.accent()` 收编进同一体系（API 不变）。
- CheckBox 受控点击拦截：`Element::checkbox(..).on_toggle(cb)`——设回调后点击/键盘激活不自动翻转
  绑定 state，交 app 决定是否翻转（可在翻转前弹确认、确认后再置真，渲染跟随 state，零闪烁）。
- `Color::lighten` / `darken` / `pick_fg`（对比自适应前景）颜色派生工具。
- 彩色 emoji 渲染：DirectWrite 字形经 `IDWriteFactory2::TranslateColorGlyphRun`
  拆成 COLR/CPAL 彩色层逐层着色（emoji、ZWJ 组合序列、肤色修饰均正确合成彩色），
  字体无彩色数据时自动回退原单色路径。新增 `examples/emoji.rs` 演示。

### Fixed
- 文本框无法输入 emoji：WM_CHAR 对补充平面字符（码点 > U+FFFF，如 emoji）
  分两条消息发来 UTF-16 代理对，原逻辑对单个代理项解码失败而丢弃。现正确
  暂存高代理项并与低代理项合成为单个 `char`，emoji 及 CJK 扩展区字符可正常输入。

## [0.1.0] - 2026-06-22

首个公开版本（Windows + macOS）。

### Added
- 核心框架：命令式 Builder API、retained 模式、DPI 感知、tiny-skia 渲染。
- 完整控件集（布局/文本/按钮/表单/容器/列表/图片/导航）、系统托盘、无边框窗口、触摸滚动、自动截屏。
- Windows 平台后端（Win32 + GDI + DirectWrite 文字）。
- macOS 平台后端（Cocoa/AppKit 窗口 + Core Text 文字 + NSPasteboard 剪贴板 + NSStatusItem 托盘）。
- 跨平台缝合层：渲染/控件/事件平台无关，平台仅实现「窗口+事件循环」与「文字引擎」两条缝。
- 开源配套：双许可（MIT OR Apache-2.0）、DCO、贡献指南、开发指南、issue/PR 模板、CI、发布工作流。

### Changed
- 依赖按 target 门控：`windows` 仅 Windows、`objc2` 系列仅 macOS。
- README 改为跨平台说明（中文主 + 英文副）。
- 依赖更新：`toml` 0.8 → 1.1；CI actions（checkout v7、action-gh-release v3）。
- **windows-rs 0.58 → 0.62 迁移**：`implement` 宏改由 `windows-core` 提供；可空句柄参数
  语义化为 `Option<T>`；`BOOL` 迁至 `windows::core`；COM 实现入参 `Option<&T>` → `Ref<'_, T>`。

[Unreleased]: https://github.com/huanfeng/wind-ui-rust/compare/v0.9.0...HEAD
[0.9.0]: https://github.com/huanfeng/wind-ui-rust/compare/v0.8.3...v0.9.0
[0.8.3]: https://github.com/huanfeng/wind-ui-rust/compare/v0.8.2...v0.8.3
[0.8.2]: https://github.com/huanfeng/wind-ui-rust/compare/v0.8.1...v0.8.2
[0.8.1]: https://github.com/huanfeng/wind-ui-rust/compare/v0.8.0...v0.8.1
[0.8.0]: https://github.com/huanfeng/wind-ui-rust/compare/v0.7.0...v0.8.0
[0.7.0]: https://github.com/huanfeng/wind-ui-rust/compare/v0.6.0...v0.7.0
[0.6.0]: https://github.com/huanfeng/wind-ui-rust/compare/v0.5.0...v0.6.0
[0.5.0]: https://github.com/huanfeng/wind-ui-rust/compare/v0.4.1...v0.5.0
[0.4.1]: https://github.com/huanfeng/wind-ui-rust/compare/v0.4.0...v0.4.1
[0.4.0]: https://github.com/huanfeng/wind-ui-rust/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/huanfeng/wind-ui-rust/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/huanfeng/wind-ui-rust/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/huanfeng/wind-ui-rust/releases/tag/v0.1.0

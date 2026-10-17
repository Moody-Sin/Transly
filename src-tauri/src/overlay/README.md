# `overlay` —— 截图翻译覆盖窗

截图翻译的会话状态与窗口生命周期。全部业务逻辑收口在本模块与 `transly-core`，
IPC 层（`commands/screenshot.rs`）只做薄转发。

## 会话状态

`ScreenshotSession(Mutex<SessionState>)`，通过 `.manage()` 注入，并以 `Arc` 共享给
`translyshot` 自定义 URI 协议的闭包。

**必须 newtype 包裹**：Tauri 按 `TypeId` 存 state，重复 `.manage()` 同类型会 panic。

| 字段 | 说明 |
|---|---|
| `generation: u32` | 会话代际，异步/跨窗口结果落地前比对。⚠️ 必须是 `u32`——specta 把 `u64` 转 BigInt，前端 `JSON.stringify` 会抛错 |
| `phase` | `Idle / Selecting / Recognizing / Result` |
| `focused_once` | 建窗瞬间「假失焦」守卫：只有先收到过 `Focused(true)`，随后的 `Focused(false)` 才可信 |
| `png` / `image_width` / `image_height` | 冻结帧字节与物理尺寸。**4K 帧约 33MB**，取消/换帧时显式清空（`Vec::new()` 才会归还堆内存） |
| `session_id` / `image_url` | 自定义协议取帧凭据与完整 URL（前端不做平台判断） |
| `window_label` | `screenshot-{generation}`，与 capabilities 的 `screenshot-*` 通配一致 |

## 关键设计

- **不透明冻结帧窗**：不是「透明窗 + 背景」，免掉对合成器的依赖（X11 无 picom 时透明窗渲染成黑），
  且截图内容与被翻译内容严格同一份像素。
- **图像字节走自定义 URI 协议**（`translyshot`），不走 IPC：4K 帧 base64 + `JSON.parse` 会让 JS 堆峰值达图像 3 倍。
  `session_id` 不匹配当前会话即 404；URL 跨平台形态由 Rust 拼好。
  ⚠️ `register_uri_scheme_protocol` 的闭包必须 `Fn + Send + Sync + 'static`，而
  `UriSchemeContext` **未实现 `Clone` 且字段私有** → **不得捕获 `&AppHandle`**，
  必须捕获 `Arc<ScreenshotSession>`（或用 `ctx.app_handle()` 动态取）。
  另：Windows/Android 的 URL 形如 `http://translyshot.localhost/...`，其余平台为
  `translyshot://localhost/...`，故 URL 一律由 Rust 拼好放进 DTO，前端不做平台判断。
- **选屏不用 tao 的 `monitor_from_point`**：其入参单位与 `cursor_position()` 的物理返回值不一致（已实测）。
  改用 `transly_core::find_monitor_for_point` 全程物理量做包含判定，找不到再回退主显示器。
- **几何用物理量**：builder 的 `position`/`inner_size` 只吃逻辑像素，故建窗时不带几何，
  建完用 `set_position` / `set_size` 传物理量。
- **窗口尺寸以捕获结果为准**：macOS Retina 下 tao 的 `Monitor::size()` 会被双重缩放（已核实源码），
  捕获图像尺寸才是真正的物理像素；不一致时以捕获结果建窗并打印诊断。

## 卡死兜底（三层）

覆盖窗是全屏置顶窗，一旦卡死会占满屏幕：

1. 建窗后 `set_focus()` 并保持 `always_on_top`；
2. `Focused(false)` 仅在 `phase == Selecting && focused_once && 已过建窗宽限期` 时取消。
   ⚠️ **两个守卫缺一不可**：`focused_once` 挡建窗**前**的假失焦；`FOCUS_GRACE`（1.2s）
   挡建窗**后**紧跟着的那次——真机实测覆盖窗加载完页面约 52ms 内必收到一次
   `Focused(false)`（`set_focus()` 的成功回调早于焦点真正稳定），少了宽限期会让
   覆盖窗「一闪即关」、功能完全不可用。
3. **60 秒看门狗**：窗口被外部 WM 挂起后 Esc 不再可信，超时强制取消；
4. `CloseRequested` → `api.prevent_close()` 并走取消路径，保证会话被清理。
   ⚠️ `WindowEvent` 是 `#[non_exhaustive]`，match 必须带 `_ => {}`。

## 与上一次会话的隔离

再次触发截图时，必须先 `hide()` + `destroy()` 旧覆盖窗**并等其像素真正从屏幕上撤下**
（`OVERLAY_SETTLE`，250ms）再捕获。`destroy()` 在 Windows 上是异步的（实测约 1.1s 后才消失），
且 **`is_visible()` 不是可靠信号**——`hide()` 一调用它就返回 false，但 DWM 合成器要再过几帧
才撤下像素。少了这段等待，连按两次快捷键的第二张冻结帧会把上一次的框选工具栏截进去。

## 已知限制

- **跨显示器框选不做**：需拼接多张不同 DPR 的图再映射归一化坐标，收益极低、错误率高。
- **只覆盖光标所在显示器**：跨屏框选未支持，同上。
- `recognize` 对选区裁剪后 OCR，块坐标经 `NormRect::map_from_subrect` 映射回整张冻结帧；
  OCR 成功但无有效文字时返回 `empty_text` 错误而非空表（空表会让前端「什么也没发生」，
  用户无法区分「没文字」与「功能坏了」）。

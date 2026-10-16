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
- **选屏不用 tao 的 `monitor_from_point`**：其入参单位与 `cursor_position()` 的物理返回值不一致（已实测）。
  改用 `transly_core::find_monitor_for_point` 全程物理量做包含判定，找不到再回退主显示器。
- **几何用物理量**：builder 的 `position`/`inner_size` 只吃逻辑像素，故建窗时不带几何，
  建完用 `set_position` / `set_size` 传物理量。
- **窗口尺寸以捕获结果为准**：macOS Retina 下 tao 的 `Monitor::size()` 会被双重缩放（已核实源码），
  捕获图像尺寸才是真正的物理像素；不一致时以捕获结果建窗并打印诊断。

## 卡死兜底（三层）

覆盖窗是全屏置顶窗，一旦卡死会占满屏幕：

1. 建窗后 `set_focus()` 并保持 `always_on_top`；
2. `Focused(false)` 仅在 `phase == Selecting && focused_once` 时取消；
3. **60 秒看门狗**：窗口被外部 WM 挂起后 Esc 不再可信，超时强制取消；
4. `CloseRequested` → `api.prevent_close()` 并走取消路径，保证会话被清理。

## 已知限制

- **跨显示器框选不做**：需拼接多张不同 DPR 的图再映射归一化坐标，收益极低、错误率高。
- 阶段 2 的 `recognize` **返回空块**，OCR 在阶段 3 落地。

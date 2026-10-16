//! 截图翻译覆盖窗的会话状态与生命周期。
//!
//! # 职责
//!
//! - 会话状态（冻结帧 PNG、显示器几何、代际、阶段）的唯一持有者；
//! - 覆盖窗的创建 / 定位 / 展示 / 销毁；
//! - 自定义 URI 协议 `translyshot` 的取帧入口；
//! - 卡死兜底：焦点守卫 + 60 秒看门狗 + `CloseRequested` 拦截。
//!
//! # 为什么冻结帧不退化成「透明窗 + 背景」
//!
//! 覆盖窗是**不透明**全屏窗，直接以冻结帧为背景。用户观感与透明窗一致
//! （原图 + 译文覆盖），但免掉透明窗对合成器的依赖（X11 无 picom 时透明窗
//! 渲染成黑），且截图内容与被翻译内容严格同一份像素，杜绝「重叠窗口已移动
//! 导致框错位」。
//!
//! # 图像字节为什么走自定义 URI 协议
//!
//! 4K 帧 = 3840×2160×4 ≈ 33 MB。走 IPC 需 base64（+33%）再 `JSON.parse`，
//! JS 堆峰值可达图像 3 倍。故走 `translyshot` 协议，`session_id` 不匹配即 404。
//! URL 由 Rust 按平台拼好放进 `ScreenshotSessionDto::image_url`，前端不做平台判断。
//!
//! # 内存纪律
//!
//! 4K 帧 33 MB，不释放就是实打实的泄漏。`png` 在 cancel / 换帧时显式清空，
//! 不依赖 GC；验收项「连续触发 5 次无内存增长」查的就是这里。

use std::sync::{Arc, Mutex, MutexGuard};

use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindowBuilder};
use transly_core::{
    find_monitor_for_point, is_capture_size_consistent, MonitorRect, NormRect, OcrBlockDto,
    OcrErrorDto, ScreenshotSessionDto,
};

/// 覆盖窗 label 前缀。
///
/// **必须**与 `capabilities/screenshot-overlay.json` 的 `windows` 通配一致
/// （`screenshot-*`），否则该窗的 `plugin:*` 调用会被 ACL 静默拒绝。
pub const OVERLAY_LABEL_PREFIX: &str = "screenshot-";

/// 看门狗超时：窗口被外部 WM 挂起（全屏置顶且不响应 Esc）时的兜底。
const WATCHDOG_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);

/// 建窗后忽略失焦事件的宽限期。
///
/// 真机实测（Windows 11 150% 缩放）：覆盖窗加载完页面后约 52ms 内会收到一次
/// `Focused(false)` —— `set_focus()` 的成功回调早于窗口真正稳定持有焦点，
/// 系统会把焦点还给上一个前台窗口。若不加宽限，`focused_once` 守卫会被这次
/// 假失焦击穿，覆盖窗「一闪即关」，功能完全不可用。
///
/// 取 1.2s：足以跨过窗口初始化与 WebView 首帧（实测 < 200ms），
/// 又短到用户真正点开其他应用时仍能及时收起。
const FOCUS_GRACE: std::time::Duration = std::time::Duration::from_millis(1200);

/// 等待旧覆盖窗真正销毁的最长时间。正常远小于此（真机实测约 1.1s 内消失）。
const OVERLAY_CLOSE_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(3000);

/// 旧覆盖窗隐藏/销毁后，再等这么久才捕获屏幕。
///
/// **`is_visible()` 不是可靠信号**：`hide()` 一调用它就返回 `false`，但 DWM 合成器
/// 要再过几帧才把窗口像素真正从屏幕上撤下。真机实测（Windows 11）不加这段等待时，
/// 连按两次快捷键的第二张冻结帧会把上一次的框选工具栏（「拖动框选要翻译的区域」
/// 与「取消（Esc）」）连同暗色背板一并截进去，且框选坐标随之整体偏移。
const OVERLAY_SETTLE: std::time::Duration = std::time::Duration::from_millis(250);

/// 轻点未拖拽的判据（物理像素）。与前端 `is_degenerate` 的阈值保持一致。
const MIN_SELECTION_PX: u32 = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Phase {
    #[default]
    Idle,
    Selecting,
    Result,
}

#[derive(Default)]
struct SessionState {
    /// 会话代际。⚠️ 必须是 `u32`：specta 把 `u64` 转成 BigInt，
    /// 前端 `JSON.stringify` 会抛错。
    generation: u32,
    phase: Phase,
    /// 建窗瞬间的「假失焦」守卫：只有先收到过 `Focused(true)`，
    /// 随后的 `Focused(false)` 才可信。
    focused_once: bool,
    /// 覆盖窗建窗时刻，用于 [`FOCUS_GRACE`] 宽限期判定。
    created_at: Option<std::time::Instant>,
    session_id: String,
    image_url: String,
    png: Vec<u8>,
    image_width: u32,
    image_height: u32,
    window_label: String,
    target_lang: String,
    engine: String,
}

impl SessionState {
    /// 释放冻结帧占用的内存。`Vec::new()` 会归还堆内存，`clear()` 不会。
    fn release_frame(&mut self) {
        self.png = Vec::new();
        self.session_id.clear();
        self.image_url.clear();
        self.image_width = 0;
        self.image_height = 0;
    }
}

/// 截图会话状态槽。
///
/// **必须 newtype 包裹**：Tauri 按 `TypeId` 存 state，重复 `.manage()` 同类型会 panic。
#[derive(Default)]
pub struct ScreenshotSession(Mutex<SessionState>);

impl ScreenshotSession {
    fn lock(&self) -> MutexGuard<'_, SessionState> {
        // 状态只是普通数据：持锁期间 panic 不会让它变成不合法状态，
        // 故中毒时取回内部数据继续用，避免一次 panic 让功能永久不可用。
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// 按平台拼接冻结帧 URL。
///
/// Windows/Android 的 WebView 只能走 `http://<scheme>.localhost/...`，
/// 其余平台走 `<scheme>://localhost/...`。
fn image_url(session_id: &str) -> String {
    if cfg!(any(target_os = "windows", target_os = "android")) {
        format!("http://translyshot.localhost/{session_id}/base.png")
    } else {
        format!("translyshot://localhost/{session_id}/base.png")
    }
}

fn close_existing_overlays(app: &AppHandle) -> Vec<tauri::WebviewWindow> {
    let stale: Vec<tauri::WebviewWindow> = app
        .webview_windows()
        .into_iter()
        .filter(|(label, _)| label.starts_with(OVERLAY_LABEL_PREFIX))
        .map(|(_, window)| window)
        .collect();
    for window in &stale {
        // 先 `hide()` 再 `destroy()`：`destroy()` 在 Windows 上是**异步**的，
        // 调用返回时窗口仍在屏幕上（真机实测 1075ms 后才从目标列表消失）。
        // 而冻结帧是在那之后立刻捕获的，旧覆盖窗会被截进新帧——
        // 用户看到的是「背景图里叠着上一次的框选工具栏」，且框选坐标随之整体偏移。
        let _ = window.hide();
        let _ = window.destroy();
    }
    stale
}

/// 等待给定窗口真正销毁。
///
/// `WebviewWindow::destroy()` 在 Windows 上是异步的：调用返回时窗口仍在屏幕上，
/// 真机实测约 1.1s 后才真正消失。**捕获必须等到旧窗的像素从屏幕上消失**，
/// 否则连按两次快捷键时，第二次的冻结帧会把第一次的覆盖窗（含框选工具栏）截进去。
async fn wait_for_windows_closed(stale: &[tauri::WebviewWindow]) {
    if stale.is_empty() {
        return;
    }
    let deadline = std::time::Instant::now() + OVERLAY_CLOSE_TIMEOUT;
    while std::time::Instant::now() < deadline {
        let still_open = stale.iter().any(|w| {
            // `is_visible` 失败（窗口已销毁）视为已关闭；成功则看是否还可见。
            w.is_visible().unwrap_or(false)
        });
        if !still_open {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    // 超时不阻断截图（宁可让用户看到一次脏帧，也不能让功能卡死），但要留痕。
    eprintln!(
        "[Screenshot] 旧覆盖窗未在 {OVERLAY_CLOSE_TIMEOUT:?} 内消失，本次冻结帧可能含残留窗口"
    );
}

/// 启动一次截图：选屏 → 捕获 → 建窗 → 展示。
pub async fn start(
    app: &AppHandle,
    target_lang: String,
    engine: String,
) -> Result<ScreenshotSessionDto, OcrErrorDto> {
    // 已有覆盖窗先隐藏并销毁，并**等其像素真正从屏幕上撤下**再捕获，
    // 否则旧窗会被截进新冻结帧（见 [`OVERLAY_SETTLE`]）。
    let stale = close_existing_overlays(app);
    wait_for_windows_closed(&stale).await;
    if !stale.is_empty() {
        tokio::time::sleep(OVERLAY_SETTLE).await;
    }

    let cursor = app
        .cursor_position()
        .map_err(|e| OcrErrorDto::capture_failed(format!("无法获取光标位置: {e}")))?;
    let monitors = app
        .available_monitors()
        .map_err(|e| OcrErrorDto::capture_failed(format!("无法枚举显示器: {e}")))?;
    if monitors.is_empty() {
        return Err(OcrErrorDto::capture_failed("未检测到任何显示器"));
    }

    let rects: Vec<MonitorRect> = monitors
        .iter()
        .map(|m| MonitorRect {
            x: m.position().x,
            y: m.position().y,
            width: m.size().width,
            height: m.size().height,
            scale_factor: m.scale_factor(),
        })
        .collect();

    // ⚠️ 不得用 tao 的 `monitor_from_point`：其入参单位与 `cursor_position()`
    // 的物理返回值不一致（已实测）。这里全程用物理量做包含判定。
    let cursor_point = (cursor.x.round() as i32, cursor.y.round() as i32);
    let primary_index = app.primary_monitor().ok().flatten().and_then(|p| {
        let (pp, ps) = (p.position(), p.size());
        rects.iter().position(|r| {
            r.x == pp.x && r.y == pp.y && r.width == ps.width && r.height == ps.height
        })
    });
    let index = find_monitor_for_point(cursor_point, &rects)
        .or(primary_index)
        .unwrap_or(0);
    let monitor = rects[index];

    // 捕获是同步阻塞调用（X11 连接、GDI 位块传输、CGImage 拷贝均可能耗时数十毫秒），
    // 必须派发到阻塞线程池，不得占用 async 工作线程。
    let capture_rect = monitor;
    let image = tauri::async_runtime::spawn_blocking(move || {
        crate::screencap::capture_monitor(&capture_rect)
    })
    .await
    .map_err(|e| OcrErrorDto::capture_failed(format!("捕获任务异常终止: {e}")))??;

    // 冻结帧的物理尺寸是唯一可信的尺寸来源：macOS Retina 下 tao 的
    // `Monitor::size()` 会把已物理化的像素再乘一次 scale（已核实 tao 0.37.1 源码），
    // 而 Windows/Linux 上两者相等。不一致时以捕获结果为准，并留下可诊断的痕迹。
    if !is_capture_size_consistent(image.width(), image.height(), monitor.width, monitor.height) {
        eprintln!(
            "[Screenshot] 捕获尺寸 {}x{} 与显示器上报 {}x{} 不一致，已以捕获尺寸为准（macOS Retina 下 tao 的 Monitor::size() 会被双重缩放）",
            image.width(),
            image.height(),
            monitor.width,
            monitor.height
        );
    }
    let (win_w, win_h) = (image.width(), image.height());

    let mut png = Vec::new();
    image
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .map_err(|e| OcrErrorDto::capture_failed(format!("冻结帧编码 PNG 失败: {e}")))?;

    let session = app.state::<Arc<ScreenshotSession>>().inner().clone();
    let (generation, label, dto) = {
        let mut st = session.lock();
        st.generation = st.generation.wrapping_add(1);
        st.phase = Phase::Selecting;
        st.focused_once = false;
        st.created_at = Some(std::time::Instant::now());
        st.session_id = format!("{:016x}", rand::random::<u64>());
        st.image_url = image_url(&st.session_id);
        st.image_width = win_w;
        st.image_height = win_h;
        st.png = png;
        st.target_lang = target_lang;
        st.engine = engine;
        // label 必须全局唯一，否则同 label 建窗会失败；代际天然递增。
        st.window_label = format!("{OVERLAY_LABEL_PREFIX}{}", st.generation);

        let dto = ScreenshotSessionDto {
            session_id: st.session_id.clone(),
            image_url: st.image_url.clone(),
            image_width: st.image_width,
            image_height: st.image_height,
            generation: st.generation,
            target_lang: st.target_lang.clone(),
            engine: st.engine.clone(),
        };
        (st.generation, st.window_label.clone(), dto)
    };

    let window = match WebviewWindowBuilder::new(
        app,
        &label,
        WebviewUrl::App(std::path::PathBuf::from("/#shot")),
    )
    .decorations(false)
    .resizable(false)
    .shadow(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .focused(true)
    .visible(false)
    .build()
    {
        Ok(window) => window,
        Err(e) => {
            // 建窗失败必须释放冻结帧：4K 帧 33MB，留着就是实打实的泄漏。
            session.lock().release_frame();
            return Err(OcrErrorDto::capture_failed(format!(
                "创建截图覆盖窗失败: {e}"
            )));
        }
    };

    // 几何一律用物理量的 `set_position` / `set_size`：builder 的
    // `position`/`inner_size` 只吃逻辑像素，会重新引入 DPI 折算。
    // 任何一步失败都必须收尾（销毁窗口 + 释放帧），否则会留下半成品满屏窗。
    let ready = (|| -> Result<(), String> {
        window
            .set_position(PhysicalPosition::new(monitor.x, monitor.y))
            .map_err(|e| format!("定位截图覆盖窗失败: {e}"))?;
        window
            .set_size(PhysicalSize::new(win_w, win_h))
            .map_err(|e| format!("设置截图覆盖窗尺寸失败: {e}"))?;
        window
            .show()
            .map_err(|e| format!("显示截图覆盖窗失败: {e}"))?;
        let _ = window.set_focus();
        Ok(())
    })();
    if let Err(message) = ready {
        let _ = window.destroy();
        session.lock().release_frame();
        return Err(OcrErrorDto::capture_failed(message));
    }

    // 覆盖窗自身的窗口事件挂在窗口上，不污染 main.rs 的全局钩子。
    let event_window = window.clone();
    window.on_window_event(move |event| handle_window_event(&event_window, event));

    // 看门狗：窗口被外部 WM 挂起后 Esc 不再可信，60 秒后强制收尾。
    let watchdog_app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(WATCHDOG_TIMEOUT).await;
        let session = watchdog_app
            .state::<Arc<ScreenshotSession>>()
            .inner()
            .clone();
        let still_waiting = {
            let st = session.lock();
            st.generation == generation && st.phase == Phase::Selecting
        };
        if still_waiting {
            cancel(&watchdog_app);
        }
    });

    Ok(dto)
}

/// 读取当前会话元数据。**幂等读**，不是一次性 take。
///
/// `React.StrictMode` 会让 effect 在开发模式跑两次；一次性 take 会让第二次
/// 挂载拿到空会话，覆盖窗表现为「一直停在背景图、无法框选」。
pub fn read_session(app: &AppHandle) -> Result<ScreenshotSessionDto, OcrErrorDto> {
    let session = app.state::<Arc<ScreenshotSession>>().inner().clone();
    let st = session.lock();
    if st.phase == Phase::Idle || st.session_id.is_empty() {
        return Err(OcrErrorDto::no_image());
    }
    Ok(ScreenshotSessionDto {
        session_id: st.session_id.clone(),
        image_url: st.image_url.clone(),
        image_width: st.image_width,
        image_height: st.image_height,
        generation: st.generation,
        target_lang: st.target_lang.clone(),
        engine: st.engine.clone(),
    })
}

/// 阶段 2 的识别入口：只做选区钳制与代际校验，**返回空块**（OCR 在阶段 3）。
pub fn recognize(
    app: &AppHandle,
    rect: NormRect,
    generation: u32,
) -> Result<Vec<OcrBlockDto>, OcrErrorDto> {
    let session = app.state::<Arc<ScreenshotSession>>().inner().clone();
    let mut st = session.lock();

    if st.phase == Phase::Idle || st.session_id.is_empty() {
        return Err(OcrErrorDto::no_image());
    }
    // 代际闸门：陈旧窗口的结果不得落到新会话上。
    if st.generation != generation {
        return Err(OcrErrorDto::no_image());
    }

    // 选区越界的唯一权威钳制点。前端的钳制是 UX 便利，这里才是信任边界。
    let clamped = rect.clamped_unit();
    if clamped.is_degenerate(st.image_width, st.image_height, MIN_SELECTION_PX) {
        // 轻点未拖拽不算错误：保持 Selecting 等待用户重新框选。
        return Ok(Vec::new());
    }

    st.phase = Phase::Result;
    Ok(Vec::new())
}

/// 取消并关闭覆盖窗，释放冻结帧内存。
pub fn cancel(app: &AppHandle) {
    let session = app.state::<Arc<ScreenshotSession>>().inner().clone();
    let label = {
        let mut st = session.lock();
        st.phase = Phase::Idle;
        st.focused_once = false;
        st.created_at = None;
        st.release_frame();
        std::mem::take(&mut st.window_label)
    };
    if !label.is_empty() {
        if let Some(window) = app.get_webview_window(&label) {
            let _ = window.destroy();
        }
    }
}

/// 供自定义 URI 协议取冻结帧；`session_id` 不匹配当前会话即拒绝。
pub fn frame_for_session(session: &ScreenshotSession, session_id: &str) -> Option<Vec<u8>> {
    let st = session.lock();
    if !session_id.is_empty() && st.session_id == session_id && !st.png.is_empty() {
        Some(st.png.clone())
    } else {
        None
    }
}

fn handle_window_event(window: &tauri::WebviewWindow, event: &tauri::WindowEvent) {
    // `WindowEvent` 是 `#[non_exhaustive]`，match 必须带 `_ => {}`。
    match event {
        tauri::WindowEvent::Focused(true) => {
            let session = window.state::<Arc<ScreenshotSession>>().inner().clone();
            session.lock().focused_once = true;
        }
        tauri::WindowEvent::Focused(false) => {
            let session = window.state::<Arc<ScreenshotSession>>().inner().clone();
            // 只在「已确认获得过焦点」「已过建窗宽限期」且仍在等待框选时取消。
            // 两个守卫缺一不可：`focused_once` 挡建窗前的假失焦，
            // `FOCUS_GRACE` 挡建窗后紧跟着的那次假失焦（真机实测 52ms 内必现）。
            let should_cancel = {
                let st = session.lock();
                let past_grace = st
                    .created_at
                    .map(|t| t.elapsed() >= FOCUS_GRACE)
                    .unwrap_or(true);
                st.phase == Phase::Selecting && st.focused_once && past_grace
            };
            if should_cancel {
                cancel(window.app_handle());
            }
        }
        tauri::WindowEvent::CloseRequested { api, .. } => {
            api.prevent_close();
            cancel(window.app_handle());
        }
        _ => {}
    }
}

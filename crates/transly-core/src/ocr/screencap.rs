//! 屏幕捕获的平台无关逻辑（纯函数，无系统调用）。
//!
//! 真实的捕获实现在 `src-tauri/src/screencap/` 下按平台分文件；
//! 本模块只放**可单测的判定与映射**，供平台层调用。

use serde::{Deserialize, Serialize};

use super::OcrErrorKind;

/// 判定当前是否为 Wayland 会话。
///
/// # 判据为何要两个环境变量
///
/// 只看 `WAYLAND_DISPLAY`：在 XWayland 兼容层下该变量存在，但应用实际
/// 渲染在 X11 上，此时 X11 捕获**可用**，误判会导致功能被无谓禁用。
/// 只看 `XDG_SESSION_TYPE`：不少发行版的 ssh/容器/tmux 环境不设置它，
/// 漏判会让 Wayland 下走进 X11 路径并静默失败。
///
/// 故以 `XDG_SESSION_TYPE` 为准（它是会话管理器给出的权威值），
/// 仅在它缺失时回落到 `WAYLAND_DISPLAY` 的存在性。
///
/// 注意：本函数只判断「会话类型」，不判断「当前是否真的能捕获」。
/// 调用方应把它与「实际能否连上 X server」结合使用。
pub fn detect_wayland_session(
    xdg_session_type: Option<&str>,
    wayland_display: Option<&str>,
) -> bool {
    match xdg_session_type {
        Some(t) if !t.trim().is_empty() => t.trim().eq_ignore_ascii_case("wayland"),
        // 会话类型未知：以 WAYLAND_DISPLAY 是否指向一个非空值作为弱判据
        _ => wayland_display
            .map(|v| !v.trim().is_empty())
            .unwrap_or(false),
    }
}

/// 把平台捕获层的原始错误信息映射为对用户有意义的错误种类。
///
/// # 为什么需要它
///
/// 平台 API 的失败信息往往是「Failed to copy data」这类对用户毫无意义的
/// 内部描述。直接归成 `internal` 会让用户看到无信息错误、也无法自助解决。
/// 本函数把可识别的失败模式翻译成**可操作**的错误种类。
///
/// 未匹配的信息一律归 `CaptureFailed` 而非 `Internal`：捕获失败本身
/// 就属于「环境问题」而非「程序缺陷」，这样用户至少能看到可重试的提示。
pub fn map_capture_error(raw: &str) -> OcrErrorKind {
    let lower = raw.to_lowercase();

    // macOS TCC 权限：CGDisplayCreateImage 返回 NULL，或系统明确报权限
    if lower.contains("permission")
        || lower.contains("not authorized")
        || lower.contains("denied")
        || lower.contains("tcc")
        || lower.contains("screen recording")
    {
        return OcrErrorKind::PermissionDenied;
    }

    // 缺组件/工具（如 tesseract 未安装）
    if lower.contains("not found")
        || lower.contains("no such file")
        || lower.contains("not installed")
        || lower.contains("missing")
    {
        return OcrErrorKind::ToolMissing;
    }

    // 无法建立显示连接：Wayland 会话、DISPLAY 未设置、X server 不可达
    if lower.contains("display")
        || lower.contains("connection")
        || lower.contains("wayland")
        || lower.contains("x server")
    {
        return OcrErrorKind::UnsupportedPlatform;
    }

    // 其余一律视为捕获失败（受保护内容、显示器休眠、合成器限制等）
    OcrErrorKind::CaptureFailed
}

/// 捕获图像尺寸与显示器声明物理尺寸之间允许的最大误差（像素）。
///
/// 差 1 像素源于驱动/窗口管理器的取整，属正常；差 2 及以上则说明单位或缩放
/// 折算出错（典型如 macOS `Monitor::size()` 在 Retina 下被双重缩放）。
pub const CAPTURE_SIZE_TOLERANCE: u32 = 1;

/// 判定捕获图像尺寸是否与显示器声明的物理尺寸吻合。
///
/// # 为什么必须做这件事
///
/// 这是**唯一**能同时兜住「上游 `Monitor` 单位改了」与「三平台 DPI 折算差异」
/// 的判据，且不引入任何平台分支假设：它只依赖「同一显示器的物理尺寸应等于
/// 捕获图像尺寸」这一不变量。
///
/// 容差取 ±1：上游各项 DPI 折算相互竞合时，差 1 像素是正常取整，
/// 一味要求完全相等会让功能在个别显卡/驱动上假性失败；放到 ±2 又会把
/// Retina 双重缩放（差整整一倍）之外的小幅折算错误一并放过。
pub fn is_capture_size_consistent(
    image_w: u32,
    image_h: u32,
    monitor_w: u32,
    monitor_h: u32,
) -> bool {
    image_w.abs_diff(monitor_w) <= CAPTURE_SIZE_TOLERANCE
        && image_h.abs_diff(monitor_h) <= CAPTURE_SIZE_TOLERANCE
}

/// 一次截图翻译会话（跨 IPC 边界传递的元数据，**不含图像字节**）。
///
/// 图像走自定义 URI 协议，故本结构只携带定位与状态信息。
/// 所有几何量均为归一化或物理量，前端不做 DPI 换算。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct ScreenshotSessionDto {
    /// 会话标识（自定义协议 URL 的一部分，防陈旧窗口读到新会话图）
    pub session_id: String,
    /// 冻结帧的完整 URL（由 Rust 按平台拼好，前端不做平台判断）
    pub image_url: String,
    /// 冻结帧的**物理**像素尺寸（供前端换算选区像素数展示）
    pub image_width: u32,
    pub image_height: u32,
    /// 会话代际（异步命令落地前比对，不等则丢弃）
    pub generation: u32,
    /// 当前目标语言（供覆盖窗初始化下拉框）
    pub target_lang: String,
    /// 当前引擎 id
    pub engine: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 关键回归：判据不能只看其中一个环境变量。
    #[test]
    fn test_detect_wayland_session_uses_authoritative_field() {
        // 会话类型明确为 wayland
        assert!(detect_wayland_session(Some("wayland"), None));
        assert!(detect_wayland_session(Some("wayland"), Some("wayland-0")));
        // 大小写与前导空白
        assert!(detect_wayland_session(Some("  Wayland  "), None));

        // 会话类型明确为 x11：即使存在 WAYLAND_DISPLAY（XWayland 兼容层）
        // 也必须判为 X11 —— 只看 WAYLAND_DISPLAY 的实现会在此误判
        assert!(
            !detect_wayland_session(Some("x11"), Some("wayland-0")),
            "X11 会话被误判为 Wayland（XWayland 下会无谓禁用功能）"
        );
        assert!(!detect_wayland_session(Some("tty"), None));
    }

    /// 关键回归：`XDG_SESSION_TYPE` 缺失时不能漏判 Wayland。
    #[test]
    fn test_detect_wayland_session_falls_back_when_field_missing() {
        assert!(detect_wayland_session(None, Some("wayland-0")));
        assert!(detect_wayland_session(Some(""), Some("wayland-1")));
        assert!(detect_wayland_session(Some("   "), Some("wayland-0")));

        assert!(!detect_wayland_session(None, None));
        assert!(!detect_wayland_session(None, Some("")));
        assert!(!detect_wayland_session(Some(""), Some("   ")));
    }

    #[test]
    fn test_map_capture_error_permission() {
        // 含权限关键词 → PermissionDenied
        for msg in [
            "screen recording permission not granted",
            "TCC denied",
            "not authorized to capture display",
            "Permission denied",
        ] {
            assert_eq!(
                map_capture_error(msg),
                OcrErrorKind::PermissionDenied,
                "{msg:?} 未按权限映射"
            );
        }
    }

    /// 关键回归：捕获失败不得归成 `Internal`（用户会看到无信息错误）。
    #[test]
    fn test_map_capture_error_never_returns_internal() {
        for msg in [
            "Failed to copy data",
            "some unknown failure",
            "",
            "GDI BitBlt failed with code 6",
            "XCB GetImage failed",
        ] {
            assert_ne!(
                map_capture_error(msg),
                OcrErrorKind::Internal,
                "{msg:?} 被归成 Internal"
            );
        }
    }

    #[test]
    fn test_map_capture_error_tool_missing() {
        assert_eq!(
            map_capture_error("tesseract: command not found"),
            OcrErrorKind::ToolMissing
        );
        assert_eq!(
            map_capture_error("No such file or directory"),
            OcrErrorKind::ToolMissing
        );
    }

    #[test]
    fn test_map_capture_error_display_connection() {
        assert_eq!(
            map_capture_error("Unable to open X display"),
            OcrErrorKind::UnsupportedPlatform
        );
        assert_eq!(
            map_capture_error("wayland session detected"),
            OcrErrorKind::UnsupportedPlatform
        );
    }

    #[test]
    fn test_map_capture_error_default_is_capture_failed() {
        assert_eq!(
            map_capture_error("BitBlt returned zero"),
            OcrErrorKind::CaptureFailed
        );
    }

    /// 关键回归：尺寸自校验的容差必须是 ±1。
    ///
    /// 容差取错（放松到 ±2，或收紧到严格相等）都会让「三平台 DPI 折算差异」
    /// 这类真缺陷漏过或被误报，且两种错法都只能靠边界值断言区分。
    #[test]
    fn test_is_capture_size_consistent_tolerance() {
        // 完全一致
        assert!(is_capture_size_consistent(1920, 1080, 1920, 1080));
        // 差 1：通过
        assert!(is_capture_size_consistent(1921, 1079, 1920, 1080));
        assert!(is_capture_size_consistent(1919, 1081, 1920, 1080));
        // 差 2：失败（容差取错时会漏过）
        assert!(!is_capture_size_consistent(1922, 1080, 1920, 1080));
        assert!(!is_capture_size_consistent(1920, 1082, 1920, 1080));
        assert!(!is_capture_size_consistent(1918, 1078, 1920, 1080));

        // macOS Retina 双重缩放：物理 3840 被上报成 7680 —— 必须拦截
        assert!(!is_capture_size_consistent(3840, 2160, 7680, 4320));
        assert!(!is_capture_size_consistent(7680, 4320, 3840, 2160));

        // 只差一个轴同样要拦
        assert!(!is_capture_size_consistent(1920, 2160, 1920, 1080));
    }

    #[test]
    fn test_screenshot_session_dto_serialization() {
        let dto = ScreenshotSessionDto {
            session_id: "abc123".to_string(),
            image_url: "translyshot://localhost/abc123/base.png".to_string(),
            image_width: 3840,
            image_height: 2160,
            generation: 1,
            target_lang: "zh-CN".to_string(),
            engine: "google".to_string(),
        };
        let json = serde_json::to_string(&dto).unwrap();
        assert!(json.contains(r#""session_id":"abc123""#));
        assert!(json.contains(r#""image_width":3840"#));
        // 字段名必须是 snake_case（与前端 bindings.ts 一致）
        assert!(json.contains("image_url"));
        assert!(json.contains("target_lang"));
    }
}

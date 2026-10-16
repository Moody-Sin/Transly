use serde::{Deserialize, Serialize};

pub mod blocks;
pub mod geom;
pub mod screencap;
pub mod tsv;

pub use blocks::{
    filter_blocks, merge_lines_into_paragraphs, merge_words_into_lines, plan_block_batches,
    OcrBlockDto, PixelBox, MIN_BLOCK_CONFIDENCE,
};
pub use geom::{find_monitor_for_point, MonitorRect, NormRect};
pub use screencap::{detect_wayland_session, map_capture_error, ScreenshotSessionDto};
pub use tsv::{parse_tesseract_tsv, tesseract_lang_arg, OcrTsvError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum OcrErrorKind {
    NoImage,
    NoLanguagePack,
    EmptyText,
    UnsupportedPlatform,
    /// 缺少必要的系统组件或外部工具（如 Linux 未安装 tesseract）
    ToolMissing,
    /// 屏幕捕获失败（受保护窗口、显示器休眠、系统限制）
    CaptureFailed,
    /// 系统未授予屏幕录制/捕获权限
    PermissionDenied,
    /// 图像或选区超出处理预算
    TooLarge,
    Internal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct OcrErrorDto {
    pub kind: OcrErrorKind,
    pub message: String,
}

impl OcrErrorDto {
    pub fn new(kind: OcrErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    pub fn no_image() -> Self {
        Self::new(OcrErrorKind::NoImage, "剪贴板中无有效图片数据")
    }

    pub fn empty_text() -> Self {
        Self::new(OcrErrorKind::EmptyText, "未能从图片中识别到有效文字")
    }

    pub fn no_language_pack() -> Self {
        Self::new(
            OcrErrorKind::NoLanguagePack,
            "系统未安装对应 OCR 语言包组件",
        )
    }

    pub fn unsupported_platform() -> Self {
        // Linux 已通过外呼 tesseract 提供 OCR（见 ocr/linux.rs），旧文案
        // 「暂未支持平台原生 OCR」在 Linux 上不会出现——该平台走 tool_missing
        // 与 unsupported_platform(Wayland) 两条路径，故此处仅覆盖 Win/mac 之外的系统。
        #[cfg(target_os = "linux")]
        let msg = "当前 Linux 会话不支持屏幕捕获（Wayland 下无法保证覆盖窗置顶），请改用『识别剪贴板图片』";
        #[cfg(not(target_os = "linux"))]
        let msg = "当前操作系统暂未支持平台原生 OCR";

        Self::new(OcrErrorKind::UnsupportedPlatform, msg)
    }

    /// Wayland 会话下的截图能力降级提示。
    ///
    /// 刻意不写「暂未支持」：那会误导用户以为永远不行。必须给出可执行的替代路径。
    pub fn wayland_unsupported() -> Self {
        Self::new(
            OcrErrorKind::UnsupportedPlatform,
            "Wayland 会话下不支持框选截图（无法保证覆盖窗置顶）。请登录 X11 会话，或改用「识别剪贴板图片」",
        )
    }

    /// 缺少外部 OCR 工具（如 Linux 未安装 tesseract），必须给出可执行的安装指引。
    pub fn tool_missing(tool: &str, hint: impl Into<String>) -> Self {
        Self::new(
            OcrErrorKind::ToolMissing,
            format!("未找到 {tool}，{}", hint.into()),
        )
    }

    /// 屏幕捕获失败（受保护窗口、DRM 内容、显示器休眠等系统级限制）。
    pub fn capture_failed(msg: impl Into<String>) -> Self {
        Self::new(OcrErrorKind::CaptureFailed, msg)
    }

    /// 未授予屏幕录制权限。文案须说明「授权后需重启应用」——macOS 的 TCC
    /// 权限在进程存活期内不会重新生效。
    pub fn permission_denied() -> Self {
        Self::new(
            OcrErrorKind::PermissionDenied,
            "未授予屏幕录制权限。请在「系统设置 → 隐私与安全性 → 屏幕录制」中允许 Transly，授权后需重启应用",
        )
    }

    /// 图像或选区超出处理预算。
    pub fn too_large() -> Self {
        Self::new(OcrErrorKind::TooLarge, "所选区域过大，请缩小范围后重试")
    }

    pub fn internal(msg: impl Into<String>) -> Self {
        Self::new(OcrErrorKind::Internal, msg)
    }
}

/// 构建去重候选识别语言列表（优先对齐源语言预期，兼顾中英双向与系统语言包）
pub fn build_candidate_languages(target_lang: &str) -> Vec<&'static str> {
    let clean = target_lang.trim().to_lowercase();
    let is_target_zh = clean == "zh"
        || clean == "zh-cn"
        || clean == "zh-hans"
        || clean == "zh-tw"
        || clean == "zh-hant";

    let mut list = Vec::with_capacity(4);
    if is_target_zh {
        // 目标是中文：优先英文与系统语言，后验中文语言包兜底
        for cand in ["en-US", "system", "zh-Hans-CN", "zh-CN"] {
            if !list.contains(&cand) {
                list.push(cand);
            }
        }
    } else {
        // 目标是非中文：优先中文与系统语言，后验英文语言包兜底
        for cand in ["zh-Hans-CN", "zh-CN", "system", "en-US"] {
            if !list.contains(&cand) {
                list.push(cand);
            }
        }
    }
    list
}

/// 原地将 RGBA8 像素切片转换为 BGRA8 字节序，避免额外全量堆分配与内存放大
pub fn rgba_to_bgra_in_place(buf: &mut [u8]) {
    for chunk in buf.as_chunks_mut::<4>().0 {
        chunk.swap(0, 2);
    }
}

/// 剪贴板位图可接受的像素总量上限（宽 × 高）。
///
/// 平台层在做任何全量像素拷贝之前必须先用它把关：RGBA 每像素 4 字节，
/// 4000 万像素即约 1.6 亿字节，再叠加缩放中间缓冲仍在可控范围；
/// 而剪贴板位图的尺寸完全由放入它的程序决定，无上限校验时
/// 一张 20000×20000 的图（1.6 GB）足以打爆进程内存。
pub const MAX_OCR_SOURCE_PIXELS: u64 = 40_000_000;

/// 判断位图尺寸是否在可处理范围内。
pub fn is_within_ocr_pixel_budget(w: u32, h: u32) -> bool {
    u64::from(w) * u64::from(h) <= MAX_OCR_SOURCE_PIXELS
}

/// 等比下采样计算：保证宽高均不大于 max_dim（防卫 Windows OCR 上限抛错）
pub fn clamp_dimensions(w: u32, h: u32, max_dim: u32) -> (u32, u32) {
    let max_side = w.max(h);
    if max_side <= max_dim || max_side == 0 {
        return (w, h);
    }
    let scale = max_dim as f64 / max_side as f64;
    let nw = ((w as f64 * scale).round() as u32).max(1);
    let nh = ((h as f64 * scale).round() as u32).max(1);
    (nw, nh)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ocr_error_dto_serialization() {
        let err = OcrErrorDto {
            kind: OcrErrorKind::NoImage,
            message: "剪贴板中无有效图片".to_string(),
        };
        let json = serde_json::to_string(&err).unwrap();
        assert!(json.contains(r#""kind":"no_image""#));
        assert!(json.contains(r#""message":"剪贴板中无有效图片""#));
    }

    #[test]
    fn test_ocr_error_dto_constructors() {
        let empty_err = OcrErrorDto::empty_text();
        assert_eq!(empty_err.kind, OcrErrorKind::EmptyText);

        let no_pack_err = OcrErrorDto::no_language_pack();
        assert_eq!(no_pack_err.kind, OcrErrorKind::NoLanguagePack);

        let unsupp_err = OcrErrorDto::unsupported_platform();
        assert_eq!(unsupp_err.kind, OcrErrorKind::UnsupportedPlatform);
    }

    #[test]
    fn test_build_candidate_languages_zh_cn() {
        let candidates = build_candidate_languages("zh-CN");
        assert_eq!(candidates, vec!["en-US", "system", "zh-Hans-CN", "zh-CN"]);
    }

    #[test]
    fn test_build_candidate_languages_non_zh() {
        let candidates = build_candidate_languages("en");
        assert_eq!(candidates, vec!["zh-Hans-CN", "zh-CN", "system", "en-US"]);
    }

    #[test]
    fn test_rgba_to_bgra_in_place_swizzle() {
        let mut buf = vec![10, 20, 30, 255, 40, 50, 60, 200];
        rgba_to_bgra_in_place(&mut buf);
        assert_eq!(buf, vec![30, 20, 10, 255, 60, 50, 40, 200]);
    }

    #[test]
    fn test_clamp_dimensions_under_limit() {
        assert_eq!(clamp_dimensions(800, 600, 2600), (800, 600));
    }

    #[test]
    fn test_clamp_dimensions_exceeding_limit() {
        let (nw, nh) = clamp_dimensions(5200, 2600, 2600);
        assert_eq!(nw, 2600);
        assert_eq!(nh, 1300);
    }

    /// 回归：剪贴板超大位图必须在任何全量像素拷贝之前被拦下。
    /// 无此校验时 20000×20000 的图会在 `to_vec()` 处申请 1.6 GB。
    #[test]
    fn test_ocr_pixel_budget_rejects_oversized_bitmap() {
        assert!(is_within_ocr_pixel_budget(800, 600));
        assert!(is_within_ocr_pixel_budget(4000, 4000)); // 恰好 1600 万
        assert!(is_within_ocr_pixel_budget(6324, 6324)); // ≈3999 万，仍在预算内

        assert!(!is_within_ocr_pixel_budget(20_000, 20_000)); // 4 亿像素
        assert!(!is_within_ocr_pixel_budget(100_000, 100_000));
        // 极窄极长（截图工具常见）同样受限，防止只按单边判断被绕过
        assert!(!is_within_ocr_pixel_budget(1, 50_000_000));
    }
}

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum OcrErrorKind {
    NoImage,
    NoLanguagePack,
    EmptyText,
    UnsupportedPlatform,
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
        #[cfg(target_os = "linux")]
        let msg =
            "Linux 系统暂未集成平台原生 OCR 引擎，建议使用 Windows / macOS 原生系统或离线翻译";
        #[cfg(not(target_os = "linux"))]
        let msg = "当前操作系统暂未支持平台原生 OCR";

        Self::new(OcrErrorKind::UnsupportedPlatform, msg)
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

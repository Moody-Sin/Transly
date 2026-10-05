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
        Self::new(
            OcrErrorKind::UnsupportedPlatform,
            "当前操作系统暂未支持平台原生 OCR",
        )
    }

    pub fn internal(msg: impl Into<String>) -> Self {
        Self::new(OcrErrorKind::Internal, msg)
    }
}

/// 构建去重候选识别语言列表（优先对齐源语言预期，避免中文界面绑定）
pub fn build_candidate_languages(target_lang: &str) -> Vec<&'static str> {
    let primary = if target_lang.trim() == "zh-CN" {
        "en-US"
    } else {
        "system"
    };

    let mut list = Vec::with_capacity(2);
    for cand in [primary, "system", "en-US"] {
        if !list.contains(&cand) {
            list.push(cand);
        }
    }
    list
}

/// 将 RGBA8 像素切片转换为 Windows SoftwareBitmap 所需的 BGRA8 字节序
pub fn rgba_to_bgra(rgba: &[u8]) -> Vec<u8> {
    let mut bgra = rgba.to_vec();
    for chunk in bgra.as_chunks_mut::<4>().0 {
        chunk.swap(0, 2);
    }
    bgra
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
        assert_eq!(candidates, vec!["en-US", "system"]);
    }

    #[test]
    fn test_build_candidate_languages_non_zh() {
        let candidates = build_candidate_languages("en");
        assert_eq!(candidates, vec!["system", "en-US"]);
    }

    #[test]
    fn test_rgba_to_bgra_swizzle() {
        let rgba = vec![10, 20, 30, 255, 40, 50, 60, 200];
        let bgra = rgba_to_bgra(&rgba);
        assert_eq!(bgra, vec![30, 20, 10, 255, 60, 50, 40, 200]);
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
}

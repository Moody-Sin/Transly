use transly_core::{OcrErrorDto, OcrErrorKind};

/// 构建去重候选识别语言列表
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
    for chunk in bgra.chunks_exact_mut(4) {
        chunk.swap(0, 2);
    }
    bgra
}

/// 等比下采样计算：保证宽高均不大于 max_dim
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

pub fn detect_language(text: &str) -> String {
    let has_cjk = text.chars().any(|c| matches!(c, '\u{4e00}'..='\u{9fff}'));
    if has_cjk {
        "zh-CN".to_string()
    } else {
        "en".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_language() {
        assert_eq!(detect_language("你好"), "zh-CN");
        assert_eq!(detect_language("Hello world"), "en");
    }
}

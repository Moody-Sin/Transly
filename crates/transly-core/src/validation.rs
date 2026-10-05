/// 验证语言代码是否合法（严格匹配支持的语言白名单）
pub fn validate_language_code(lang: &str) -> bool {
    let clean = lang.trim();
    if clean.is_empty() || clean.len() > 10 {
        return false;
    }

    matches!(
        clean.to_lowercase().as_str(),
        "auto"
            | "zh"
            | "zh-cn"
            | "zh-tw"
            | "zh-hans"
            | "zh-hant"
            | "en"
            | "en-us"
            | "en-gb"
            | "ja"
            | "ko"
            | "fr"
            | "de"
            | "es"
            | "ru"
            | "it"
            | "pt"
            | "nl"
            | "pl"
            | "ar"
            | "sv"
            | "tr"
            | "vi"
            | "uk"
    )
}

/// 验证翻译引擎标识是否合法
pub fn validate_engine_id(engine: &str) -> bool {
    let clean = engine.trim().to_lowercase();
    if clean.is_empty() {
        return true; // 允许默认引擎
    }
    matches!(
        clean.as_str(),
        "google" | "google_web" | "deepl" | "openai" | "deepseek"
    )
}

/// 验证翻译请求参数合法性
pub fn validate_translation_input(
    text: &str,
    source_lang: &str,
    target_lang: &str,
) -> Result<(), &'static str> {
    if text.is_empty() {
        return Err("翻译文本不能为空");
    }
    if text.chars().count() > 10_000 {
        return Err("翻译文本过长 (单次最大限制 10,000 字符)");
    }
    if !source_lang.is_empty() && !validate_language_code(source_lang) {
        return Err("无效或不受支持的源语言代码");
    }
    if !validate_language_code(target_lang) || target_lang.eq_ignore_ascii_case("auto") {
        return Err("无效或不受支持的目标语言代码");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_engine_id_validation() {
        assert!(validate_engine_id("google"));
        assert!(validate_engine_id("deepl"));
        assert!(validate_engine_id("deepseek"));
        assert!(validate_engine_id("openai"));
        assert!(validate_engine_id(""));

        assert!(!validate_engine_id("unknown_engine"));
        assert!(!validate_engine_id("baidu"));
    }

    #[test]
    fn test_language_code_validation() {
        assert!(validate_language_code("auto"));
        assert!(validate_language_code("zh-CN"));
        assert!(validate_language_code("en"));
        assert!(validate_language_code("ja"));
        assert!(validate_language_code("fr"));

        assert!(!validate_language_code(""));
        assert!(!validate_language_code("a"));
        assert!(!validate_language_code(
            "very_long_invalid_language_code_injection"
        ));
        assert!(!validate_language_code("zh<script>"));
    }

    #[test]
    fn test_translation_input_validation() {
        assert!(validate_translation_input("Hello", "auto", "zh-CN").is_ok());
        assert!(validate_translation_input("", "auto", "zh-CN").is_err());
        assert!(validate_translation_input("Hello", "auto", "auto").is_err());
        assert!(validate_translation_input("Hello", "invalid#lang", "zh-CN").is_err());
    }
}

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

/// 验证翻译引擎标识是否合法。
///
/// 注意区分两套 ID 空间，勿混为一谈：
/// - **别名**：调用方（前端下拉框、HTTP 请求 `engine` 字段）传入的引擎选择，
///   取值为 `google` / `google_web` / `deepl` / `openai` / `deepseek`，即此处的白名单。
/// - **实例 ID**：引擎池为具体实例生成的标识，LLM 引擎形如 `openai:gpt-4o-mini`
///   （见 `OpenAIEngine`），会出现在响应 `engine` 字段与缓存 key 中。
///
/// 两者都不该流经本函数的校验：校验只针对**外部可控的入参**，实例 ID 是内部
/// 派生值且含用户自定义模型名，放进白名单只会凭空扩大注入面而无收益。
pub fn validate_engine_id(engine: &str) -> bool {
    let clean = engine.trim().to_lowercase();
    if clean.is_empty() {
        return true; // 允许默认引擎
    }
    matches!(
        clean.as_str(),
        "google" | "google_web" | "deepl" | "openai" | "deepseek" | "offline"
    )
}

/// 验证翻译语言对合法性（源语言 + 目标语言）。
///
/// IPC 与本地 HTTP 服务共用此函数："不支持该语言"只允许有一个判定来源，
/// 两处各写一份必然漂移。调用方自行把失败映射为本层的错误形态。
pub fn validate_language_pair(source_lang: &str, target_lang: &str) -> crate::Result<()> {
    if !source_lang.is_empty() && !validate_language_code(source_lang) {
        return Err(crate::Error::Input("无效或不受支持的源语言代码".into()));
    }
    if !validate_language_code(target_lang) || target_lang.eq_ignore_ascii_case("auto") {
        return Err(crate::Error::Input("无效或不受支持的目标语言代码".into()));
    }
    Ok(())
}

/// 验证翻译请求参数合法性。
///
/// 返回 `Error::Input` 而非裸字符串：调用方（IPC / HTTP）统一经 `classify_error`
/// 分类，不各自拼装错误 DTO，避免新增校验项时漏改某一条路径。
pub fn validate_translation_input(
    text: &str,
    source_lang: &str,
    target_lang: &str,
) -> crate::Result<()> {
    if text.is_empty() {
        return Err(crate::Error::Input("翻译文本不能为空".into()));
    }
    if text.chars().count() > crate::MAX_ITEM_CHARS {
        return Err(crate::Error::Input(format!(
            "翻译文本过长 (单次最大限制 {} 字符)",
            crate::MAX_ITEM_CHARS
        )));
    }
    validate_language_pair(source_lang, target_lang)
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
        // 校验失败必须是 Input 类：调用方据此判定不可重试
        assert_eq!(
            validate_translation_input("", "auto", "zh-CN")
                .unwrap_err()
                .kind(),
            crate::ErrorKind::Input
        );
    }

    #[test]
    fn test_language_pair_shared_by_ipc_and_http() {
        // 只校验语言对，不涉及文本：HTTP 批量入口据此拒绝非法语言参数
        assert!(validate_language_pair("auto", "zh-CN").is_ok());
        assert!(validate_language_pair("en", "ja").is_ok());
        assert!(validate_language_pair("auto", "auto").is_err());
        assert!(validate_language_pair("invalid#lang", "zh-CN").is_err());
        assert!(validate_language_pair("auto", "zh<script>").is_err());
    }
}

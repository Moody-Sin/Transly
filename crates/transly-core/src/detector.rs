//! 离线语言检测与智能双向语言路由
//!
//! 采用纯 Rust 基于 Unicode 字符区段分布的高性能统计分析，
//! 单次判定开销在微秒级别，零外部网络依赖与额外内存体积。

/// 检测文本的主要语言代码（如 "zh-CN", "en", "ja", "ko", "ru", "ar"）。
/// 若文本为空或无足够有效文字字符（如纯标点数字），返回 `None`。
pub fn detect_language(text: &str) -> Option<&'static str> {
    let mut cjk_count = 0usize;
    let mut kana_count = 0usize;
    let mut hangul_count = 0usize;
    let mut latin_count = 0usize;
    let mut cyrillic_count = 0usize;
    let mut arabic_count = 0usize;

    for ch in text.chars() {
        match ch {
            // 日文平假名 & 片假名
            '\u{3040}'..='\u{309F}' | '\u{30A0}'..='\u{30FF}' => kana_count += 1,
            // 韩文谚文音节与字母
            '\u{AC00}'..='\u{D7AF}' | '\u{1100}'..='\u{11FF}' | '\u{3130}'..='\u{318F}' => {
                hangul_count += 1
            }
            // 汉字 (CJK 统一表意文字)
            '\u{4E00}'..='\u{9FFF}' | '\u{3400}'..='\u{4DBF}' | '\u{F900}'..='\u{FAFF}' => {
                cjk_count += 1
            }
            // 西里尔字母 (俄语等)
            '\u{0400}'..='\u{04FF}' => cyrillic_count += 1,
            // 阿拉伯文字
            '\u{0600}'..='\u{06FF}' => arabic_count += 1,
            // 拉丁字母
            'a'..='z' | 'A'..='Z' | '\u{00C0}'..='\u{024F}' => latin_count += 1,
            _ => {}
        }
    }

    let total_chars =
        cjk_count + kana_count + hangul_count + latin_count + cyrillic_count + arabic_count;
    if total_chars == 0 {
        return None;
    }

    // 1. 日文假名特征极强：日文混合汉字时往往假名占比较高或出现即可高度确认日文
    if kana_count > 0 {
        return Some("ja");
    }

    // 2. 韩文谚文特征极强
    if hangul_count > 0 && hangul_count >= cjk_count {
        return Some("ko");
    }

    // 3. 阿拉伯文
    if arabic_count > 0 && arabic_count >= total_chars / 2 {
        return Some("ar");
    }

    // 4. 俄语/西里尔文
    if cyrillic_count > 0 && cyrillic_count >= total_chars / 2 {
        return Some("ru");
    }

    // 5. 中文与英文等主要语系比对
    // 注意：汉字单个字符蕴含的信息密度远高于单个拉丁字母（一个词通常仅 2~4 个汉字，对应英文 5~15 个字母）。
    // 在技术文档、代码注释场景（如："请使用 resolve_smart_target_lang 进行推断"），
    // 汉字字符虽然较少，但语义主体明确是中文。
    // 因此给予 CJK 汉字 4.0 倍权重折算，兼顾纯英文与中英夹杂长标识符技术文本准确识别。
    let weighted_cjk = (cjk_count as f64) * 4.0;
    let latin_score = latin_count as f64;

    if weighted_cjk > latin_score {
        Some("zh-CN")
    } else if latin_count > 0 {
        Some(detect_latin_sublanguage(text))
    } else {
        None
    }
}

/// 针对拉丁字母文本细分检测主要西欧语系（法、德、西等），避免全部无差别归入英文
fn detect_latin_sublanguage(text: &str) -> &'static str {
    let mut de_score = 0usize;
    let mut fr_score = 0usize;
    let mut es_score = 0usize;

    for ch in text.chars() {
        match ch {
            'ä' | 'Ä' | 'ö' | 'Ö' | 'ü' | 'Ü' | 'ß' => de_score += 3,
            'é' | 'É' | 'è' | 'È' | 'ê' | 'Ê' | 'à' | 'À' | 'ç' | 'Ç' | 'ù' | 'Ù' | 'ô' | 'Ô' => {
                fr_score += 3
            }
            'ñ' | 'Ñ' | '¿' | '¡' => es_score += 3,
            _ => {}
        }
    }

    for word in text.split(|c: char| !c.is_alphabetic()) {
        if word.is_empty() {
            continue;
        }
        let w = word.to_lowercase();
        match w.as_str() {
            "bonjour" | "merci" | "oui" | "avec" | "dans" | "pour" | "vous" | "nous" | "elle"
            | "sont" => fr_score += 5,
            "guten" | "danke" | "bitte" | "nicht" | "oder" | "aber" | "sehr" | "wenn" | "auch" => {
                de_score += 5
            }
            "hola" | "gracias" | "amigo" | "buenos" | "dias" | "como" | "pero" | "todos" => {
                es_score += 5
            }
            _ => {}
        }
    }

    if de_score > 0 && de_score >= fr_score && de_score >= es_score {
        "de"
    } else if fr_score > 0 && fr_score >= de_score && fr_score >= es_score {
        "fr"
    } else if es_score > 0 {
        "es"
    } else {
        "en"
    }
}

/// 根据输入的源文本与用户当前配置的目标语言，智能推断应翻译的目标语言。
///
/// 典型应用场景：
/// - 用户默认目标语言为中文（zh-CN）：当输入中文时，自动对转为英文（en）；当输入英文或其它外文时，保持翻译为中文（zh-CN）。
/// - 用户默认目标语言为英文（en）：当输入英文时，自动对转为中文（zh-CN）；当输入非英文时，保持翻译为英文（en）。
pub fn resolve_smart_target_lang(source_text: &str, user_target_lang: &str) -> String {
    let clean_target = user_target_lang.trim();
    let detected = match detect_language(source_text) {
        Some(d) => d,
        None => return clean_target.to_string(),
    };

    let is_target_chinese = is_chinese_code(clean_target);
    let is_target_english = is_english_code(clean_target);

    if is_target_chinese {
        if detected == "zh-CN" {
            "en".to_string()
        } else {
            clean_target.to_string()
        }
    } else if is_target_english {
        if detected == "en" {
            "zh-CN".to_string()
        } else {
            clean_target.to_string()
        }
    } else {
        // 其他目标语言（如日文、法文等）：
        // 若检测到的源语言恰好与目标语言一致，则反向对转为中文或英文
        if detected.eq_ignore_ascii_case(clean_target) {
            "zh-CN".to_string()
        } else {
            clean_target.to_string()
        }
    }
}

fn is_chinese_code(lang: &str) -> bool {
    matches!(
        lang.to_lowercase().as_str(),
        "zh" | "zh-cn" | "zh-hans" | "zh-tw" | "zh-hant"
    )
}

fn is_english_code(lang: &str) -> bool {
    matches!(lang.to_lowercase().as_str(), "en" | "en-us" | "en-gb")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_language_chinese() {
        assert_eq!(detect_language("你好，世界！这是一个测试"), Some("zh-CN"));
        assert_eq!(detect_language("使用 Rust 编写高性能软件"), Some("zh-CN"));
        // 中英夹杂技术文档/代码注释：英文标识符字母多但主体仍为中文
        assert_eq!(
            detect_language("请使用 resolve_smart_target_lang 进行推断"),
            Some("zh-CN")
        );
    }

    #[test]
    fn test_detect_language_english() {
        assert_eq!(detect_language("Hello world, this is a test."), Some("en"));
        assert_eq!(
            detect_language("Fast and lightweight translation tool"),
            Some("en")
        );
    }

    #[test]
    fn test_detect_language_japanese_and_korean() {
        assert_eq!(detect_language("こんにちは世界"), Some("ja"));
        assert_eq!(detect_language("안녕하세요 세계"), Some("ko"));
    }

    #[test]
    fn test_detect_language_empty_and_symbols() {
        assert_eq!(detect_language(""), None);
        assert_eq!(detect_language("   123456 !!! ??? --- "), None);
    }

    #[test]
    fn test_detect_language_russian_and_arabic() {
        assert_eq!(detect_language("Привет мир"), Some("ru"));
        assert_eq!(detect_language("مرحبا بالعالم"), Some("ar"));
    }

    #[test]
    fn test_resolve_smart_target_lang_when_user_target_is_chinese() {
        // 用户默认目标是中文：输入中文自动对转英文，输入外文保持中文
        assert_eq!(resolve_smart_target_lang("你好世界", "zh-CN"), "en");
        assert_eq!(resolve_smart_target_lang("Hello world", "zh-CN"), "zh-CN");
        assert_eq!(resolve_smart_target_lang("こんにちは", "zh-CN"), "zh-CN");
        assert_eq!(resolve_smart_target_lang("Привет мир", "zh-CN"), "zh-CN");
    }

    #[test]
    fn test_resolve_smart_target_lang_when_user_target_is_english() {
        // 用户默认目标是英文：输入英文自动对转中文，输入中文保持英文
        assert_eq!(resolve_smart_target_lang("Hello world", "en"), "zh-CN");
        assert_eq!(resolve_smart_target_lang("你好世界", "en"), "en");
        assert_eq!(resolve_smart_target_lang("こんにちは", "en"), "en");
    }

    #[test]
    fn test_detect_language_european_latin() {
        assert_eq!(detect_language("Bonjour tout le monde"), Some("fr"));
        assert_eq!(detect_language("Guten Tag, wie geht es Ihnen?"), Some("de"));
        assert_eq!(detect_language("Hola amigo, buenos dias"), Some("es"));
        assert_eq!(detect_language("über Straße"), Some("de"));
    }

    #[test]
    fn test_resolve_smart_target_lang_european_when_target_is_english() {
        // 用户目标语言为英文，输入法文或德文时，意图为外翻英，必须保持目标为英文，不得误转为中文
        assert_eq!(
            resolve_smart_target_lang("Bonjour tout le monde", "en"),
            "en"
        );
        assert_eq!(resolve_smart_target_lang("Guten Tag", "en"), "en");
        assert_eq!(resolve_smart_target_lang("Hola amigo", "en"), "en");
    }

    #[test]
    fn test_resolve_smart_target_lang_fallback_on_unrecognized() {
        assert_eq!(resolve_smart_target_lang("12345 ???", "zh-CN"), "zh-CN");
        assert_eq!(resolve_smart_target_lang("", "en"), "en");
    }
}

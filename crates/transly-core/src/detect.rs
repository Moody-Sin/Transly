pub fn detect_language(text: &str) -> String {
    // 优先检测常见东亚字符集
    let mut has_cjk = false;
    let mut has_kana = false;
    let mut has_hangul = false;

    for c in text.chars() {
        match c {
            // 日文平假名 & 片假名
            '\u{3040}'..='\u{309f}' | '\u{30a0}'..='\u{30ff}' => {
                has_kana = true;
                break;
            }
            // 韩文字母 / 音节
            '\u{ac00}'..='\u{d7af}' | '\u{1100}'..='\u{11ff}' | '\u{3130}'..='\u{318f}' => {
                has_hangul = true;
                break;
            }
            // 汉字 (CJK Unified Ideographs)
            '\u{4e00}'..='\u{9fff}' => {
                has_cjk = true;
            }
            _ => {}
        }
    }

    if has_kana {
        "ja".to_string()
    } else if has_hangul {
        "ko".to_string()
    } else if has_cjk {
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
        assert_eq!(detect_language("你好，世界"), "zh-CN");
        assert_eq!(detect_language("こんにちは"), "ja");
        assert_eq!(detect_language("안녕하세요"), "ko");
        assert_eq!(detect_language("Hello world"), "en");
    }
}

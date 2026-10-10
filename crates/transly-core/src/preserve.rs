//! Markdown 与代码块保护模块
//!
//! 在翻译前提取多行代码块、行内代码与数学公式，替换为 Unicode 专有括线占位符，
//! 翻译完成后精确还原，防止翻译引擎将代码和变量名误翻或破坏格式。

pub struct PreservedText {
    pub text: String,
    pub stash: Vec<String>,
}

/// 提取文本中的代码块并用安全占位符替换
#[allow(clippy::while_let_on_iterator)]
pub fn stash_code_blocks(input: &str) -> PreservedText {
    let mut stash = Vec::new();
    let mut output = String::with_capacity(input.len());
    let mut chars = input.char_indices().peekable();

    while let Some((idx, ch)) = chars.next() {
        // 1. 检查多行代码块: ```
        if ch == '`' && input[idx..].starts_with("```") {
            let start = idx;
            // 跳过开头的 ```
            chars.next();
            chars.next();

            let mut end = None;
            while let Some((i, c)) = chars.next() {
                if c == '`' && input[i..].starts_with("```") {
                    // 跳过结尾的 ```
                    chars.next();
                    chars.next();
                    end = Some(i + 3);
                    break;
                }
            }

            let block_end = end.unwrap_or(input.len());
            let code_content = &input[start..block_end];
            let placeholder = format!("⟦TRANSLY_CODE_{}⟧", stash.len());
            stash.push(code_content.to_string());
            output.push_str(&placeholder);
            continue;
        }

        // 2. 检查行内代码: `
        if ch == '`' {
            let start = idx;
            let mut end = None;
            let mut inner_chars = chars.clone();
            while let Some((i, c)) = inner_chars.next() {
                if c == '`' {
                    end = Some(i + 1);
                    break;
                } else if c == '\n' {
                    // 行内代码不跨行
                    break;
                }
            }

            if let Some(e) = end {
                // 推进主迭代器到 e 对应的字符
                while let Some(&(i, _)) = chars.peek() {
                    if i < e {
                        chars.next();
                    } else {
                        break;
                    }
                }
                let code_content = &input[start..e];
                let placeholder = format!("⟦TRANSLY_CODE_{}⟧", stash.len());
                stash.push(code_content.to_string());
                output.push_str(&placeholder);
                continue;
            } else {
                // 未闭合，作为普通字符
                output.push('`');
                continue;
            }
        }

        output.push(ch);
    }

    PreservedText {
        text: output,
        stash,
    }
}

/// 将占位符精确还原为原代码块，包含对大模型偶发空格变形的容错兜底
pub fn restore_code_blocks(input: &str, stash: &[String]) -> String {
    let mut result = input.to_string();
    // 1. 优先执行完全精确匹配替换
    for (idx, original) in stash.iter().enumerate() {
        let placeholder = format!("⟦TRANSLY_CODE_{idx}⟧");
        result = result.replace(&placeholder, original);
    }

    // 2. 二级容错：大模型翻译时偶发在特殊括线内引入空格（如 ⟦ TRANSLY_CODE_0 ⟧）
    if result.contains("⟦") && result.contains("⟧") {
        if let Ok(re) = regex::Regex::new(r"⟦\s*TRANSLY_CODE_(\d+)\s*⟧") {
            result = re
                .replace_all(&result, |caps: &regex::Captures| {
                    if let Some(m) = caps.get(1) {
                        if let Ok(idx) = m.as_str().parse::<usize>() {
                            if let Some(orig) = stash.get(idx) {
                                return orig.clone();
                            }
                        }
                    }
                    caps.get(0).map(|c| c.as_str()).unwrap_or("").to_string()
                })
                .into_owned();
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stash_and_restore_multiline_code_block() {
        let input = "请阅读以下代码：\n```rust\nfn main() {\n    println!(\"Hello\");\n}\n```\n这是说明文字。";
        let preserved = stash_code_blocks(input);
        assert_eq!(preserved.stash.len(), 1);
        assert!(preserved.text.contains("⟦TRANSLY_CODE_0⟧"));
        assert!(!preserved.text.contains("println!"));

        // 模拟翻译（占位符保持不变）
        let translated = preserved
            .text
            .replace("请阅读以下代码：", "Please read the code:");
        let restored = restore_code_blocks(&translated, &preserved.stash);
        assert!(restored.starts_with("Please read the code:"));
        assert!(restored.contains("```rust\nfn main() {\n    println!(\"Hello\");\n}\n```"));
    }

    #[test]
    fn test_stash_and_restore_inline_code() {
        let input = "调用 `transly_core::detect_language` 获取语种，使用 `Option<String>` 返回。";
        let preserved = stash_code_blocks(input);
        assert_eq!(preserved.stash.len(), 2);
        assert_eq!(preserved.stash[0], "`transly_core::detect_language`");
        assert_eq!(preserved.stash[1], "`Option<String>`");

        let restored = restore_code_blocks(&preserved.text, &preserved.stash);
        assert_eq!(restored, input);
    }

    #[test]
    fn test_restore_with_llm_whitespace_variation() {
        let stash = vec![
            "`let x = 1;`".to_string(),
            "```\nprintln!();\n```".to_string(),
        ];
        // 模拟 LLM 引入空格的译文
        let translated = "Here is ⟦ TRANSLY_CODE_0 ⟧ and block ⟦TRANSLY_CODE_1 ⟧.";
        let restored = restore_code_blocks(translated, &stash);
        assert_eq!(
            restored,
            "Here is `let x = 1;` and block ```\nprintln!();\n```."
        );
    }

    #[test]
    fn test_unclosed_backtick_handled_gracefully() {
        let input = "这里有一个未闭合的 `反引号字符";
        let preserved = stash_code_blocks(input);
        assert_eq!(preserved.stash.len(), 0);
        assert_eq!(preserved.text, input);
    }
}

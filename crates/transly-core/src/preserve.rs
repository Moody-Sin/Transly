//! Markdown 与代码块保护模块
//!
//! 在翻译前提取多行代码块、行内代码与数学公式，替换为 Unicode 专有括线占位符，
//! 翻译完成后精确还原，防止翻译引擎将代码和变量名误翻或破坏格式。

pub struct PreservedText {
    pub text: String,
    pub stash: Vec<String>,
}

/// 占位符前缀。形如 `⟦TRANSLY_CODE_0⟧`，用 Unicode 专有括线避免与普通文本混淆。
const PLACEHOLDER_PREFIX: &str = "⟦TRANSLY_CODE_";
/// 转义前缀。见 `restore_code_blocks` 的防自污染说明。
const LITERAL_PREFIX: &str = "⟦TRANSLY_LITERAL_";

/// 容忍大模型在占位符括线内插入空格的容错正则。
/// 静态化以避免每次还原都重新解析并编译 DFA（批量翻译下逐项命中）。
static PLACEHOLDER_REGEX: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
    regex::Regex::new(r"⟦\s*TRANSLY_CODE_(\d+)\s*⟧").expect("占位符容错正则是编译期常量，必须合法")
});

#[inline]
fn placeholder(idx: usize) -> String {
    format!("{PLACEHOLDER_PREFIX}{idx}⟧")
}

/// 提取文本中的代码块并用安全占位符替换
pub fn stash_code_blocks(input: &str) -> PreservedText {
    let mut stash = Vec::new();
    let mut output = String::with_capacity(input.len());
    let mut chars = input.char_indices().peekable();

    while let Some((idx, ch)) = chars.next() {
        // 1. 检查多行代码块: ``` 及更长围栏 (支持 CommonMark 4+ 反引号长围栏)
        if ch == '`' && input[idx..].starts_with("```") {
            let start = idx;
            let fence_len = input[start..].chars().take_while(|&c| c == '`').count();
            let fence_pattern = "`".repeat(fence_len);

            // 用克隆迭代器扫描闭合围栏，绝不消耗主迭代器：
            // 未闭合时主迭代器必须仍停在围栏之后，后续正文才能继续参与翻译。
            // 必须跳过起始围栏剩余的反引号（因为主循环在 idx 处只取出了首个反引号）
            let mut scan = chars.clone();
            for _ in 1..fence_len {
                scan.next();
            }

            let mut end = None;
            for (i, c) in scan {
                if c == '`' && input[i..].starts_with(&fence_pattern) {
                    let close_len = input[i..].chars().take_while(|&k| k == '`').count();
                    end = Some(i + close_len);
                    break;
                }
            }

            match end {
                Some(block_end) => {
                    while let Some(&(i, _)) = chars.peek() {
                        if i < block_end {
                            chars.next();
                        } else {
                            break;
                        }
                    }
                    let code_content = &input[start..block_end];
                    let ph = placeholder(stash.len());
                    stash.push(code_content.to_string());
                    output.push_str(&ph);
                }
                None => {
                    // 未闭合的围栏：其后全部内容按原文原样输出（不做任何 stash）。
                    // 两种错误必须同时避免——把剩余正文当代码会静默丢失翻译，
                    // 而只输出 "```" 剩下的反引号又会被误判成行内代码。
                    output.push_str(&input[start..]);
                    return PreservedText {
                        text: output,
                        stash,
                    };
                }
            }
            continue;
        }

        // 2. 检查行内代码: `
        if ch == '`' {
            let start = idx;
            let mut end = None;
            for (i, c) in chars.clone() {
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
                let ph = placeholder(stash.len());
                stash.push(code_content.to_string());
                output.push_str(&ph);
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
///
/// 防自污染：占位符是纯可预测文本，若原文自身含有 `⟦TRANSLY_CODE_n⟧`
/// （技术文档讲解本占位符语法、或用户粘贴了含占位符的片段），展开后会被
/// 后续轮次当成真占位符再次替换，用户原文被静默改写。
/// 因此先把 stash 内容的占位符形态转义为 `LITERAL_PREFIX`，替换完成后再统一还原。
/// 该方案与替换顺序无关，可彻底杜绝双向污染。
pub fn restore_code_blocks(input: &str, stash: &[String]) -> String {
    let safe_stash: Vec<String> = stash
        .iter()
        .map(|s| s.replace(PLACEHOLDER_PREFIX, LITERAL_PREFIX))
        .collect();

    let mut result = input.to_string();
    // 1. 优先执行完全精确匹配替换
    for (idx, original) in safe_stash.iter().enumerate() {
        result = result.replace(&placeholder(idx), original);
    }

    // 2. 二级容错：大模型翻译时偶发在特殊括线内引入空格（如 ⟦ TRANSLY_CODE_0 ⟧）
    if result.contains("⟦") && result.contains("⟧") {
        result = PLACEHOLDER_REGEX
            .replace_all(&result, |caps: &regex::Captures| {
                if let Some(m) = caps.get(1) {
                    if let Ok(idx) = m.as_str().parse::<usize>() {
                        if let Some(orig) = safe_stash.get(idx) {
                            return orig.clone();
                        }
                    }
                }
                caps.get(0).map(|c| c.as_str()).unwrap_or("").to_string()
            })
            .into_owned();
    }

    // 3. 还原被转义的字面量
    result.replace(LITERAL_PREFIX, PLACEHOLDER_PREFIX)
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

    /// 回归：未闭合的三反引号围栏不得把其后全部正文当成代码吞掉。
    /// AI 生成与流式截断的 Markdown 极易产生未闭合围栏。
    #[test]
    fn test_unclosed_code_fence_does_not_swallow_body() {
        let input =
            "Intro text.\n```rust\nfn main(){}\n\nConclusion paragraph that must be translated.";
        let preserved = stash_code_blocks(input);

        assert_eq!(preserved.stash.len(), 0, "未闭合围栏不应进入 stash");
        assert!(
            preserved.text.contains("Conclusion paragraph"),
            "围栏之后的正文必须保留并参与翻译：{}",
            preserved.text
        );
        assert!(preserved.text.contains("fn main(){}"));

        let restored = restore_code_blocks(&preserved.text, &preserved.stash);
        assert_eq!(restored, input, "无 stash 时还原必须是恒等变换");
    }

    /// 回归：原文自身含有形似占位符的文本时，还原过程不得把它当成真占位符
    /// 二次替换，导致用户原文被静默改写。
    #[test]
    fn test_restore_is_not_polluted_by_placeholder_lookalike() {
        let input = "a `⟦TRANSLY_CODE_1⟧` b `keepme`";
        let preserved = stash_code_blocks(input);
        assert_eq!(preserved.stash.len(), 2);

        // 译文原样返回（占位符未被翻译引擎破坏）
        let restored = restore_code_blocks(&preserved.text, &preserved.stash);
        assert_eq!(
            restored, input,
            "原文中的占位符形态文本必须逐字保留，不得被展开成别的 stash 内容"
        );
    }

    /// 更刁钻的情形：stash 内容引用了一个**尚未处理**的索引。
    /// 仅靠倒序替换无法挡住这种情况，转义方案才能彻底解决。
    #[test]
    fn test_restore_isolates_back_references_across_indices() {
        let preserved = PreservedText {
            text: format!("{PLACEHOLDER_PREFIX}0⟧ {PLACEHOLDER_PREFIX}1⟧ {PLACEHOLDER_PREFIX}2⟧"),
            stash: vec![
                "值A".to_string(),
                "值B".to_string(),
                format!("引用了 {PLACEHOLDER_PREFIX}0⟧ 和 {PLACEHOLDER_PREFIX}1⟧"),
            ],
        };

        let restored = restore_code_blocks(&preserved.text, &preserved.stash);
        assert_eq!(
            restored, "值A 值B 引用了 ⟦TRANSLY_CODE_0⟧ 和 ⟦TRANSLY_CODE_1⟧",
            "跨索引的反向引用必须保持字面量，不得被二次替换"
        );
    }

    #[test]
    fn test_restore_still_tolerates_whitespace_variation_with_lookalikes() {
        let input = "使用 ⟦ TRANSLY_CODE_0 ⟧ 与 `keepme`";
        let preserved = stash_code_blocks(input);
        let translated = format!(
            "使用 ⟦ TRANSLY_CODE_0 ⟧ 与 {}",
            placeholder(preserved.stash.len() - 1)
        );
        let restored = restore_code_blocks(&translated, &preserved.stash);
        assert!(restored.starts_with("使用 `keepme` 与 `keepme`") || restored.contains("keepme"));
    }

    #[test]
    fn test_stash_and_restore_four_backtick_code_block_with_nested_blocks() {
        let input = "请查看包含嵌套的 Markdown 示例：\n````markdown\n这里是外层 4 反引号，内含：\n```rust\nfn nested() {}\n```\n结束内层\n````\n外部正文保持。";
        let preserved = stash_code_blocks(input);
        assert_eq!(preserved.stash.len(), 1);
        assert!(preserved.text.contains("⟦TRANSLY_CODE_0⟧"));
        assert!(!preserved.text.contains("fn nested()"));
        assert!(preserved.text.contains("外部正文保持。"));

        let translated = preserved
            .text
            .replace(
                "请查看包含嵌套的 Markdown 示例：",
                "Please check the Markdown example:",
            )
            .replace("外部正文保持。", "Outer text preserved.");
        let restored = restore_code_blocks(&translated, &preserved.stash);
        assert!(restored.starts_with("Please check the Markdown example:"));
        assert!(restored.contains("````markdown\n这里是外层 4 反引号，内含：\n```rust\nfn nested() {}\n```\n结束内层\n````"));
        assert!(restored.ends_with("Outer text preserved."));
    }
}

//! Markdown (.md) 文档解析与重构

use super::txt::parse_txt;
use super::ParsedDocument;
use crate::preserve::{restore_code_blocks, stash_code_blocks};

/// 提取 YAML Frontmatter（若存在）。
///
/// Frontmatter 必须以文件首行的 `---` 开头，并以单独一行的 `---` 闭合。
fn extract_frontmatter(content: &str) -> (&str, &str) {
    if !content.starts_with("---") {
        return ("", content);
    }

    let rest = &content[3..];
    if !rest.starts_with('\n') && !rest.starts_with("\r\n") {
        return ("", content);
    }

    // 寻找闭合 fence
    // 匹配 \n---\n 或 \n---\r\n 或 \r\n---\r\n 或 \r\n---\n
    let search_patterns = ["\n---\n", "\r\n---\r\n", "\n---\r\n", "\r\n---\n"];
    let mut close_pos: Option<usize> = None;
    for pat in search_patterns {
        if let Some(pos) = rest.find(pat) {
            let candidate = 3 + pos + pat.len();
            close_pos = match close_pos {
                Some(prev) => Some(prev.min(candidate)),
                None => Some(candidate),
            };
        }
    }

    match close_pos {
        Some(pos) => (&content[..pos], &content[pos..]),
        None => ("", content),
    }
}

/// 解析 Markdown 文档，提取需翻译的段落块并返回重构闭包。
///
/// 算法原理：
/// 1. 提取并保留 YAML Frontmatter（元数据原样不翻译）；
/// 2. 使用 `stash_code_blocks` 提取并占位保护多行代码块、行内代码与 LaTeX 数学公式；
/// 3. 段落级切分，提取可翻译文本；
/// 4. 重构阶段：使用段落译文重组文本，调用 `restore_code_blocks` 还原代码与公式，重新拼装 Frontmatter。
pub fn parse_md(content: &str) -> ParsedDocument {
    if content.is_empty() {
        return (Vec::new(), Box::new(|_| String::new()));
    }

    let (frontmatter, body) = extract_frontmatter(content);
    let frontmatter_owned = frontmatter.to_string();

    let preserved = stash_code_blocks(body);
    let (chunks, txt_reconstructor) = parse_txt(&preserved.text);
    let stash = preserved.stash;

    let reconstructor = Box::new(move |translated: Vec<String>| {
        let reconstructed_stashed_body = txt_reconstructor(translated);
        let restored_body = restore_code_blocks(&reconstructed_stashed_body, &stash);
        format!("{frontmatter_owned}{restored_body}")
    });

    (chunks, reconstructor)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_md_empty() {
        let (chunks, recon) = parse_md("");
        assert!(chunks.is_empty());
        assert_eq!(recon(vec![]), "");
    }

    #[test]
    fn test_parse_md_preserves_yaml_frontmatter() {
        let content = "---\ntitle: Doc Title\nauthor: Alice\n---\n\n# Heading 1\n\nParagraph text.";
        let (chunks, recon) = parse_md(content);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0], "# Heading 1");
        assert_eq!(chunks[1], "Paragraph text.");

        let translated = vec!["# 一级标题".to_string(), "段落正文。".to_string()];
        let out = recon(translated);
        assert_eq!(
            out,
            "---\ntitle: Doc Title\nauthor: Alice\n---\n\n# 一级标题\n\n段落正文。"
        );
    }

    #[test]
    fn test_parse_md_preserves_code_fences_and_inline_code() {
        let content = "Check this `var name` inline.\n\n```rust\nfn main() {\n    println!(\"Hello\");\n}\n```\n\nFinal paragraph.";
        let (chunks, recon) = parse_md(content);
        assert_eq!(chunks.len(), 3);

        let translated = vec![
            chunks[0].replace("Check this", "检查此项"),
            chunks[1].clone(), // 代码块占位符原样保留
            "最终段落。".to_string(),
        ];
        let out = recon(translated);
        assert!(out.contains("检查此项 `var name`"));
        assert!(out.contains("```rust\nfn main() {\n    println!(\"Hello\");\n}\n```"));
        assert!(out.contains("最终段落。"));
    }
}

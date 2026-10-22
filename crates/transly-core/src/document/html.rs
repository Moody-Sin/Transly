//! HTML 网页文档解析与重构
//!
//! 提取 HTML 标签外部的正文文本节点进行翻译，
//! 严格保留所有标签名、属性（class/id/href）、注释以及 script/style 内容。

use super::ParsedDocument;

#[derive(Debug, Clone)]
enum HtmlSegment {
    TagOrRaw(String),
    Text(usize),
}

/// 解析 HTML 内容，提取正文文本并返回重构闭包。
pub fn parse_html(content: &str) -> ParsedDocument {
    if content.trim().is_empty() {
        return (Vec::new(), Box::new(|_| String::new()));
    }

    let mut segments = Vec::new();
    let mut chunks = Vec::new();
    let mut chars = content.char_indices().peekable();
    let mut last_idx = 0;

    let mut in_script = false;
    let mut in_style = false;

    while let Some(&(idx, ch)) = chars.peek() {
        if ch == '<' {
            // 先将标签前的文本收集（若在普通正文区）
            if idx > last_idx {
                let text_slice = &content[last_idx..idx];
                if in_script || in_style || text_slice.trim().is_empty() {
                    segments.push(HtmlSegment::TagOrRaw(text_slice.to_string()));
                } else {
                    let chunk_idx = chunks.len();
                    chunks.push(text_slice.to_string());
                    segments.push(HtmlSegment::Text(chunk_idx));
                }
            }

            // 扫描完整的 HTML 标签或注释
            let tag_start = idx;
            chars.next(); // 消费 '<'

            // 判断是否是注释 <!-- ... -->
            if content[tag_start..].starts_with("<!--") {
                let comment_end = content[tag_start..]
                    .find("-->")
                    .map(|pos| tag_start + pos + 3)
                    .unwrap_or(content.len());

                while let Some(&(i, _)) = chars.peek() {
                    if i < comment_end {
                        chars.next();
                    } else {
                        break;
                    }
                }
                segments.push(HtmlSegment::TagOrRaw(
                    content[tag_start..comment_end].to_string(),
                ));
                last_idx = comment_end;
                continue;
            }

            let mut tag_end = tag_start + 1;
            for (i, c) in chars.by_ref() {
                if c == '>' {
                    tag_end = i + 1;
                    break;
                }
            }

            let tag_str = &content[tag_start..tag_end];
            let lower_tag = tag_str.to_lowercase();

            // 检查进入或退出 script/style
            if lower_tag.starts_with("<script") {
                in_script = true;
            } else if lower_tag.starts_with("</script") {
                in_script = false;
            } else if lower_tag.starts_with("<style") {
                in_style = true;
            } else if lower_tag.starts_with("</style") {
                in_style = false;
            }

            segments.push(HtmlSegment::TagOrRaw(tag_str.to_string()));
            last_idx = tag_end;
        } else {
            chars.next();
        }
    }

    if last_idx < content.len() {
        let tail = &content[last_idx..];
        if in_script || in_style || tail.trim().is_empty() {
            segments.push(HtmlSegment::TagOrRaw(tail.to_string()));
        } else {
            let chunk_idx = chunks.len();
            chunks.push(tail.to_string());
            segments.push(HtmlSegment::Text(chunk_idx));
        }
    }

    let content_len = content.len();
    let reconstructor = Box::new(move |translated: Vec<String>| {
        let mut out = String::with_capacity(content_len);
        for seg in segments {
            match seg {
                HtmlSegment::TagOrRaw(raw) => out.push_str(&raw),
                HtmlSegment::Text(idx) => {
                    if let Some(t) = translated.get(idx) {
                        out.push_str(t);
                    }
                }
            }
        }
        out
    });

    (chunks, reconstructor)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_html_preserves_tags_and_scripts() {
        let html = r#"<!DOCTYPE html>
<html>
<head>
  <style>body { color: red; }</style>
  <script>const x = "do not translate";</script>
</head>
<body>
  <h1>Page Title</h1>
  <p class="intro">Welcome to our site.</p>
</body>
</html>"#;

        let (chunks, recon) = parse_html(html);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0], "Page Title");
        assert_eq!(chunks[1], "Welcome to our site.");

        let translated = vec!["页面标题".to_string(), "欢迎来到我们的网站。".to_string()];
        let out = recon(translated);

        assert!(out.contains("<h1>页面标题</h1>"));
        assert!(out.contains("<p class=\"intro\">欢迎来到我们的网站。</p>"));
        assert!(out.contains("const x = \"do not translate\";"));
        assert!(out.contains("body { color: red; }"));
    }
}

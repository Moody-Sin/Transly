//! 纯文本 (.txt) 文档解析与重构

use super::ParsedDocument;

#[derive(Debug, Clone)]
enum TxtSegment {
    Delim(String),
    Text(usize),
}

/// 解析纯文本，提取段落内容并返回重构闭包。
///
/// 算法原理：
/// 寻找连续非空白行作为翻译块（段落），段落之间的换行符与空白作为定界符保留。
/// 重构时使用相同定界符拼接译文，实现 100% 格式还原（包括 CRLF/LF 与多个连续换行）。
pub fn parse_txt(content: &str) -> ParsedDocument {
    if content.is_empty() {
        return (Vec::new(), Box::new(|_| String::new()));
    }

    let mut segments = Vec::new();
    let mut chunks = Vec::new();

    let mut chars = content.char_indices().peekable();
    let mut last_idx = 0;

    while let Some(&(idx, ch)) = chars.peek() {
        // 判断是否是换行符定界区
        if ch == '\r' || ch == '\n' {
            let start = idx;
            let mut end = idx;
            while let Some(&(i, c)) = chars.peek() {
                if c == '\r' || c == '\n' || c == ' ' || c == '\t' {
                    chars.next();
                    end = i + c.len_utf8();
                } else {
                    break;
                }
            }
            // 只有当定界符内包含至少一个换行符时才视为段落分隔符
            let delim_slice = &content[start..end];
            if delim_slice.contains('\n') || delim_slice.contains('\r') {
                segments.push(TxtSegment::Delim(delim_slice.to_string()));
                last_idx = end;
            } else {
                // 仅普通行内空格，继续
            }
        } else {
            // 正文段落：直到遇到连续两个换行符或行首空行
            let start = last_idx;
            let mut end = last_idx;
            while let Some(&(i, c)) = chars.peek() {
                if c == '\r' || c == '\n' {
                    // 查看换行之后是否又是换行（即空行分割）
                    let mut lookahead = chars.clone();
                    lookahead.next(); // 消费当前的 \r 或 \n
                    if let Some(&(_, next_c)) = lookahead.peek() {
                        if c == '\r' && next_c == '\n' {
                            lookahead.next(); // 消费 \n
                        }
                    }
                    // 检查下下个字符是否是换行或空白行
                    let is_paragraph_end = if let Some(&(_, after_eol)) = lookahead.peek() {
                        after_eol == '\r' || after_eol == '\n'
                    } else {
                        true // 文件末尾
                    };

                    if is_paragraph_end {
                        end = i;
                        break;
                    } else {
                        // 仅段内普通单换行，继续作为同段内容
                        chars.next();
                        end = i + c.len_utf8();
                    }
                } else {
                    chars.next();
                    end = i + c.len_utf8();
                }
            }

            let text_slice = &content[start..end];
            if !text_slice.is_empty() {
                let chunk_idx = chunks.len();
                chunks.push(text_slice.to_string());
                segments.push(TxtSegment::Text(chunk_idx));
                last_idx = end;
            }
        }
    }

    let content_len = content.len();
    let reconstructor = Box::new(move |translated: Vec<String>| {
        let mut out = String::with_capacity(content_len);
        for seg in segments {
            match seg {
                TxtSegment::Delim(d) => out.push_str(&d),
                TxtSegment::Text(idx) => {
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
    fn test_parse_txt_empty() {
        let (chunks, recon) = parse_txt("");
        assert!(chunks.is_empty());
        assert_eq!(recon(vec![]), "");
    }

    #[test]
    fn test_parse_txt_basic_paragraphs() {
        let content = "Hello world.\nThis is line 2.\n\nSecond paragraph.\n\nThird paragraph.";
        let (chunks, recon) = parse_txt(content);
        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0], "Hello world.\nThis is line 2.");
        assert_eq!(chunks[1], "Second paragraph.");
        assert_eq!(chunks[2], "Third paragraph.");

        let translated = vec![
            "你好世界。\n这是第二行。".to_string(),
            "第二段。".to_string(),
            "第三段。".to_string(),
        ];
        let out = recon(translated);
        assert_eq!(out, "你好世界。\n这是第二行。\n\n第二段。\n\n第三段。");
    }

    #[test]
    fn test_parse_txt_preserves_crlf_and_trailing_newlines() {
        let content = "\r\nParagraph 1\r\n\r\nParagraph 2\r\n\r\n";
        let (chunks, recon) = parse_txt(content);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0], "Paragraph 1");
        assert_eq!(chunks[1], "Paragraph 2");

        let translated = vec!["段落 1".to_string(), "段落 2".to_string()];
        let out = recon(translated);
        assert_eq!(out, "\r\n段落 1\r\n\r\n段落 2\r\n\r\n");
    }
}

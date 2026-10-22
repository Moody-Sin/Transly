//! SubRip (.srt) 字幕解析与重构

use super::ParsedDocument;

#[derive(Debug, Clone)]
struct SrtEntry {
    index_line: String,
    timecode_line: String,
    original_text: String,
}

/// 检测内容采用的主要换行符格式（CRLF 或 LF）
fn detect_eol(content: &str) -> &'static str {
    if content.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    }
}

/// 解析 SubRip (.srt) 字幕文件，提取字幕文本并返回重构闭包。
///
/// 算法原理与边界保护：
/// 1. 严格锁定序号行与时间轴行（`00:00:00,000 --> 00:00:00,000`），100% 原样保留；
/// 2. 提取每个字幕块中的多行文字，将其聚合成单个待翻译块（保留内部换行），送往翻译流水线；
/// 3. 重构时保持原序号、原时间轴和原格式顺序，仅回填翻译后的字幕文本；
/// 4. 自动保留文件原有的换行风格（CRLF / LF）。
pub fn parse_srt(content: &str) -> ParsedDocument {
    if content.trim().is_empty() {
        return (Vec::new(), Box::new(|_| String::new()));
    }

    let eol = detect_eol(content);
    let mut entries = Vec::new();

    // 统一规范化换行切分
    let raw_lines: Vec<&str> = content.lines().collect();
    let mut i = 0;

    while i < raw_lines.len() {
        // 跳过空行
        while i < raw_lines.len() && raw_lines[i].trim().is_empty() {
            i += 1;
        }
        if i >= raw_lines.len() {
            break;
        }

        let first_line = raw_lines[i].trim();

        // 检查下一行是否是时间轴行（包含 -->）
        if i + 1 < raw_lines.len() && raw_lines[i + 1].contains("-->") {
            let index_line = first_line.to_string();
            let timecode_line = raw_lines[i + 1].trim().to_string();
            i += 2;

            let mut text_lines = Vec::new();
            while i < raw_lines.len() && !raw_lines[i].trim().is_empty() {
                // 如果紧跟着又是一行看起来像数字且下一行是时间轴，则停止（兼容无空行分割的异常 SRT）
                if i + 1 < raw_lines.len()
                    && raw_lines[i].trim().parse::<u32>().is_ok()
                    && raw_lines[i + 1].contains("-->")
                {
                    break;
                }
                text_lines.push(raw_lines[i]);
                i += 1;
            }

            entries.push(SrtEntry {
                index_line,
                timecode_line,
                original_text: text_lines.join(eol),
            });
        } else if first_line.contains("-->") {
            // 容错：个别字幕文件遗漏了序号行，首行即时间轴
            let index_line = (entries.len() + 1).to_string();
            let timecode_line = first_line.to_string();
            i += 1;

            let mut text_lines = Vec::new();
            while i < raw_lines.len() && !raw_lines[i].trim().is_empty() {
                if i + 1 < raw_lines.len()
                    && (raw_lines[i].trim().parse::<u32>().is_ok() || raw_lines[i].contains("-->"))
                    && raw_lines[i + 1].contains("-->")
                {
                    break;
                }
                text_lines.push(raw_lines[i]);
                i += 1;
            }

            entries.push(SrtEntry {
                index_line,
                timecode_line,
                original_text: text_lines.join(eol),
            });
        } else {
            // 其它无法识别的孤立行，跳过
            i += 1;
        }
    }

    let chunks: Vec<String> = entries.iter().map(|e| e.original_text.clone()).collect();

    let content_len = content.len();
    let reconstructor = Box::new(move |translated: Vec<String>| {
        let mut out = String::with_capacity(content_len);
        for (idx, entry) in entries.into_iter().enumerate() {
            out.push_str(&entry.index_line);
            out.push_str(eol);
            out.push_str(&entry.timecode_line);
            out.push_str(eol);

            let trans = translated.get(idx).cloned().unwrap_or(entry.original_text);
            out.push_str(&trans);
            out.push_str(eol);
            out.push_str(eol);
        }
        out
    });

    (chunks, reconstructor)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_srt_empty() {
        let (chunks, recon) = parse_srt("");
        assert!(chunks.is_empty());
        assert_eq!(recon(vec![]), "");
    }

    #[test]
    fn test_parse_srt_standard_entries() {
        let content = "\
1
00:00:01,000 --> 00:00:04,000
Hello world!
This is line 2.

2
00:00:05,500 --> 00:00:08,200
Enjoy your movie!
";
        let (chunks, recon) = parse_srt(content);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0], "Hello world!\nThis is line 2.");
        assert_eq!(chunks[1], "Enjoy your movie!");

        let translated = vec![
            "你好，世界！\n这是第二行。".to_string(),
            "祝您观影愉快！".to_string(),
        ];
        let out = recon(translated);
        let expected = "\
1
00:00:01,000 --> 00:00:04,000
你好，世界！
这是第二行。

2
00:00:05,500 --> 00:00:08,200
祝您观影愉快！

";
        assert_eq!(out, expected);
    }

    #[test]
    fn test_parse_srt_crlf_preservation() {
        let content = "1\r\n00:00:01,000 --> 00:00:04,000\r\nSub text\r\n\r\n2\r\n00:00:05,000 --> 00:00:07,000\r\nAnother sub\r\n\r\n";
        let (chunks, recon) = parse_srt(content);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0], "Sub text");
        assert_eq!(chunks[1], "Another sub");

        let translated = vec!["字幕文本".to_string(), "另一条字幕".to_string()];
        let out = recon(translated);
        assert!(out.contains("00:00:01,000 --> 00:00:04,000\r\n字幕文本\r\n\r\n"));
        assert!(out.contains("00:00:05,000 --> 00:00:07,000\r\n另一条字幕\r\n\r\n"));
    }
}

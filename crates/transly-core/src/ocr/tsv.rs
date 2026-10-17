//! tesseract TSV 输出解析（纯字符串处理）。
//!
//! # TSV 格式
//!
//! 表头 12 列：
//! ```text
//! level page_num block_num par_num line_num word_num left top width height conf text
//! ```
//! `level` 取值 1(page) / 2(block) / 3(par) / 4(line) / 5(word)。
//! 我们只取 `level == 5` 的词级结果，再交由 `blocks.rs` 做几何聚合。
//!
//! `conf` 是 **0–100 的整数**（非 0–1），`-1` 表示该行无有效识别结果。
//! `text` 列是最后一列，**可能含制表符残留**，故必须用 `splitn(12, '\t')`
//! 而非 `split('\t')` —— 后者会把含制表符的文本截断。

use super::blocks::PixelBox;

/// TSV 解析错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OcrTsvError {
    /// 输出完全为空（无表头也无数据行）
    Empty,
    /// 缺少表头行（前 12 列列名不匹配）
    MissingHeader,
}

impl std::fmt::Display for OcrTsvError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OcrTsvError::Empty => write!(f, "tesseract 未输出任何内容"),
            OcrTsvError::MissingHeader => write!(f, "tesseract 输出缺少 TSV 表头"),
        }
    }
}

impl std::error::Error for OcrTsvError {}

const COLUMN_COUNT: usize = 12;

/// 解析 tesseract 的 TSV 输出为词级包围盒。
///
/// 容错策略：字段解析失败或列数不足的**单行直接丢弃**，绝不 panic ——
/// tesseract 在不同版本/语言包下会输出额外诊断行，因一行异常而整图失败
/// 是不可接受的。
pub fn parse_tesseract_tsv(tsv: &str) -> Result<Vec<PixelBox>, OcrTsvError> {
    let mut lines = tsv.lines();

    // 跳过可能的空行后校验表头
    let header = loop {
        match lines.next() {
            Some(l) if l.trim().is_empty() => continue,
            Some(l) => break l,
            None => return Err(OcrTsvError::Empty),
        }
    };
    if !is_tsv_header(header) {
        return Err(OcrTsvError::MissingHeader);
    }

    let mut words = Vec::new();
    for line in lines {
        if line.trim().is_empty() {
            continue;
        }
        let mut cols = line.splitn(COLUMN_COUNT, '\t');

        // 取 12 列；任一缺失即丢弃该行
        let Some(level) = cols.next() else { continue };
        let Some(_page) = cols.next() else { continue };
        let Some(_block) = cols.next() else { continue };
        let Some(_par) = cols.next() else { continue };
        let Some(_line) = cols.next() else { continue };
        let Some(_word) = cols.next() else { continue };
        let Some(left) = cols.next() else { continue };
        let Some(top) = cols.next() else { continue };
        let Some(width) = cols.next() else { continue };
        let Some(height) = cols.next() else { continue };
        let Some(conf) = cols.next() else { continue };
        // 最后一列是文本，可能为空（纯空白词）
        let text = cols.next().unwrap_or("");

        // 只取词级
        let Ok(level) = level.trim().parse::<u32>() else {
            continue;
        };
        if level != 5 {
            continue;
        }

        // 置信度：-1 表示无有效结果，直接丢弃
        let Ok(conf) = conf.trim().parse::<f32>() else {
            continue;
        };
        if conf < 0.0 {
            continue;
        }
        // 归一化到 0–1（tesseract 用 0–100）
        let confidence = (conf / 100.0).clamp(0.0, 1.0);

        let (Ok(left), Ok(top), Ok(width), Ok(height)) = (
            left.trim().parse::<u32>(),
            top.trim().parse::<u32>(),
            width.trim().parse::<u32>(),
            height.trim().parse::<u32>(),
        ) else {
            continue;
        };

        // 零尺寸词无法渲染
        if width == 0 || height == 0 {
            continue;
        }

        // 文本可能保留尾部 \r（Windows 换行）
        let text = text.trim_end_matches('\r');
        if text.trim().is_empty() {
            continue;
        }

        words.push(PixelBox {
            x: left,
            y: top,
            w: width,
            h: height,
            text: text.to_string(),
            confidence: Some(confidence),
        });
    }

    Ok(words)
}

/// 校验首行是否为 tesseract 的 TSV 表头。
///
/// 只检查前若干列名，不比对全部 12 列——tesseract 版本间的列名表述
/// （如 `page_num` / `page`）存在细微差异，全量比对会导致误判。
fn is_tsv_header(line: &str) -> bool {
    let cols: Vec<&str> = line.split('\t').collect();
    if cols.len() < COLUMN_COUNT {
        return false;
    }
    let expected = [
        "level",
        "page_num",
        "block_num",
        "par_num",
        "line_num",
        "word_num",
        "left",
        "top",
        "width",
        "height",
        "conf",
    ];
    expected
        .iter()
        .zip(cols.iter())
        .all(|(e, a)| a.trim().eq_ignore_ascii_case(e))
}

/// 把目标语言映射为 tesseract 的 `-l` 语言包名。
///
/// ⚠️ **不能复用 `build_candidate_languages`** —— 它产出的 `"system"` 是
/// Windows OCR 的语义（"用系统已装语言"），传给 tesseract 的 `-l` 会直接失败。
///
/// 只覆盖本项目支持的目标语言；其余回落英文（tesseract 的 `eng` 是最普及的包）。
pub fn tesseract_lang_arg(target_lang: &str) -> &'static str {
    let clean = target_lang.trim().to_lowercase();
    // 中文（简繁均用 chi_sim：chi_tra 安装率低，且简体模型对繁体文本仍有可用识别率）
    if clean.starts_with("zh") {
        return "chi_sim";
    }
    if clean.starts_with("ja") {
        return "jpn";
    }
    if clean.starts_with("ko") {
        return "kor";
    }
    if clean.starts_with("en") {
        return "eng";
    }
    "eng"
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEADER: &str = "level\tpage_num\tblock_num\tpar_num\tline_num\tword_num\tleft\ttop\twidth\theight\tconf\ttext";

    fn row(level: u32, left: u32, top: u32, w: u32, h: u32, conf: &str, text: &str) -> String {
        format!("{level}\t1\t1\t1\t1\t1\t{left}\t{top}\t{w}\t{h}\t{conf}\t{text}")
    }

    #[test]
    fn test_parse_basic() {
        let tsv = format!(
            "{HEADER}\n{}\n{}\n",
            row(5, 10, 20, 30, 15, "95.5", "Hello"),
            row(5, 50, 20, 40, 15, "88", "world"),
        );
        let words = parse_tesseract_tsv(&tsv).unwrap();
        assert_eq!(words.len(), 2);
        assert_eq!(words[0].text, "Hello");
        assert_eq!(
            (words[0].x, words[0].y, words[0].w, words[0].h),
            (10, 20, 30, 15)
        );
        assert!((words[0].confidence.unwrap() - 0.955).abs() < 1e-5);
        assert!((words[1].confidence.unwrap() - 0.88).abs() < 1e-5);
    }

    /// 关键回归：必须用 `splitn(12)`，否则含制表符的文本会被截断。
    #[test]
    fn test_parse_preserves_tabs_in_text_column() {
        // 文本列含制表符（tesseract 偶发输出）
        let line = "5\t1\t1\t1\t1\t1\t10\t20\t30\t15\t90\tfoo\tbar";
        let tsv = format!("{HEADER}\n{line}\n");
        let words = parse_tesseract_tsv(&tsv).unwrap();
        assert_eq!(words.len(), 1, "含制表符的行被整行丢弃了");
        assert_eq!(
            words[0].text, "foo\tbar",
            "文本被截断为 {:?}（split('\\t') 的典型症状）",
            words[0].text
        );
    }

    #[test]
    fn test_parse_skips_non_word_levels() {
        let tsv = format!(
            "{HEADER}\n{}\n{}\n{}\n{}\n",
            row(1, 0, 0, 100, 100, "95", "page text"),
            row(2, 0, 0, 100, 100, "95", "block text"),
            row(4, 0, 0, 100, 100, "95", "line text"),
            row(5, 10, 20, 30, 15, "95", "word"),
        );
        let words = parse_tesseract_tsv(&tsv).unwrap();
        assert_eq!(words.len(), 1, "只应保留 level==5 的词");
        assert_eq!(words[0].text, "word");
    }

    #[test]
    fn test_parse_drops_negative_confidence() {
        let tsv = format!(
            "{HEADER}\n{}\n{}\n",
            row(5, 10, 20, 30, 15, "-1", "bogus"),
            row(5, 50, 20, 30, 15, "80", "real"),
        );
        let words = parse_tesseract_tsv(&tsv).unwrap();
        assert_eq!(words.len(), 1);
        assert_eq!(words[0].text, "real");
    }

    /// 容错：单行字段异常只丢该行，不影响整图。
    #[test]
    fn test_parse_tolerates_malformed_rows() {
        let tsv = format!(
            "{HEADER}\n{}\n{}\n{}\n{}\n{}\n",
            row(5, 10, 20, 30, 15, "95", "good1"),
            "5\t1\t1\t1\t1\t1\tNOT_A_NUMBER\t20\t30\t15\t95\tbad",
            "5\t1\t1\t1\t1\t1\t10\t20\t30", // 列数不足
            "garbage line without tabs",
            row(5, 50, 20, 30, 15, "85", "good2"),
        );
        let words = parse_tesseract_tsv(&tsv).unwrap();
        assert_eq!(words.len(), 2, "异常行应被丢弃而非影响正常行");
        assert_eq!(words[0].text, "good1");
        assert_eq!(words[1].text, "good2");
    }

    #[test]
    fn test_parse_drops_zero_size_and_empty_text() {
        let tsv = format!(
            "{HEADER}\n{}\n{}\n{}\n",
            row(5, 10, 20, 0, 15, "95", "zerowidth"),
            row(5, 10, 20, 30, 0, "95", "zeroheight"),
            row(5, 10, 20, 30, 15, "95", "   "),
        );
        let words = parse_tesseract_tsv(&tsv).unwrap();
        assert!(words.is_empty());
    }

    #[test]
    fn test_parse_handles_crlf_and_blank_lines() {
        let tsv = format!(
            "{HEADER}\r\n\r\n{}\r\n",
            row(5, 10, 20, 30, 15, "95", "word")
        );
        let words = parse_tesseract_tsv(&tsv).unwrap();
        assert_eq!(words.len(), 1);
        assert_eq!(words[0].text, "word", "尾部 \\r 应被剥除");
    }

    #[test]
    fn test_parse_empty_input() {
        assert_eq!(parse_tesseract_tsv(""), Err(OcrTsvError::Empty));
        assert_eq!(parse_tesseract_tsv("\n\n  \n"), Err(OcrTsvError::Empty));
    }

    #[test]
    fn test_parse_missing_header() {
        let tsv = format!("{}\n", row(5, 10, 20, 30, 15, "95", "word"));
        assert_eq!(parse_tesseract_tsv(&tsv), Err(OcrTsvError::MissingHeader));
    }

    #[test]
    fn test_parse_header_case_insensitive() {
        let header = "Level\tPage_Num\tBlock_Num\tPar_Num\tLine_Num\tWord_Num\tLeft\tTop\tWidth\tHeight\tConf\tText";
        let tsv = format!("{header}\n{}\n", row(5, 10, 20, 30, 15, "95", "word"));
        assert_eq!(parse_tesseract_tsv(&tsv).unwrap().len(), 1);
    }

    #[test]
    fn test_confidence_clamped_to_unit_range() {
        // 异常的超范围置信度应被钳制而非产生 >1 的值
        let tsv = format!("{HEADER}\n{}\n", row(5, 10, 20, 30, 15, "150", "over"));
        let words = parse_tesseract_tsv(&tsv).unwrap();
        assert_eq!(words[0].confidence, Some(1.0));
    }

    /// 关键回归：`system` 是 Windows OCR 的语义，绝不能传给 tesseract 的 `-l`。
    #[test]
    fn test_tesseract_lang_arg_never_returns_system() {
        for lang in [
            "zh-CN", "zh", "zh-TW", "ZH-cn", "en", "en-US", "ja", "ko", "system", "auto", "",
            "unknown",
        ] {
            let arg = tesseract_lang_arg(lang);
            assert_ne!(arg, "system", "语言 {lang:?} 被错误映射为 system");
            assert!(!arg.is_empty());
        }
    }

    #[test]
    fn test_tesseract_lang_arg_mappings() {
        assert_eq!(tesseract_lang_arg("zh-CN"), "chi_sim");
        assert_eq!(tesseract_lang_arg("zh-Hant"), "chi_sim");
        assert_eq!(tesseract_lang_arg("en"), "eng");
        assert_eq!(tesseract_lang_arg("en-US"), "eng");
        assert_eq!(tesseract_lang_arg("ja"), "jpn");
        assert_eq!(tesseract_lang_arg("ko"), "kor");
        // 未支持语言回落英文
        assert_eq!(tesseract_lang_arg("fr"), "eng");
        assert_eq!(tesseract_lang_arg(""), "eng");
        // 大小写与前导空白
        assert_eq!(tesseract_lang_arg("  ZH-CN  "), "chi_sim");
    }
}

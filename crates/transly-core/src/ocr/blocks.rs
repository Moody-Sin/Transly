//! OCR 块几何聚合与批量规划（纯逻辑）。
//!
//! # 为什么必须先合并再翻译
//!
//! Windows 的 `OcrWord` 与 tesseract 的 `level==5` 都是**单词级**产物。
//! 直接逐词送翻译会因上下文缺失导致语序崩坏（与 LESSONS 第 8 条
//! 「按碎片处理导致整体语义错乱」同源）。故先按几何把词聚成行、行聚成段，
//! 再以段为单位送翻译。

use serde::{Deserialize, Serialize};

use super::geom::NormRect;

/// 像素级包围盒（OCR 引擎的原始产物，单位是**喂给 OCR 那张图**的像素）。
#[derive(Debug, Clone, PartialEq)]
pub struct PixelBox {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
    pub text: String,
    /// 置信度 ∈ [0,1]，引擎不提供时为 `None`（Windows 不提供）
    pub confidence: Option<f32>,
}

impl PixelBox {
    pub fn new(x: u32, y: u32, w: u32, h: u32, text: impl Into<String>) -> Self {
        Self {
            x,
            y,
            w,
            h,
            text: text.into(),
            confidence: None,
        }
    }

    pub fn right(&self) -> u32 {
        self.x.saturating_add(self.w)
    }

    pub fn bottom(&self) -> u32 {
        self.y.saturating_add(self.h)
    }

    pub fn center_y(&self) -> u32 {
        self.y.saturating_add(self.h / 2)
    }

    /// 文本次序合并：`a` 在前，`b` 紧随其后。
    fn merge_with(&self, other: &PixelBox) -> PixelBox {
        let x0 = self.x.min(other.x);
        let y0 = self.y.min(other.y);
        let x1 = self.right().max(other.right());
        let y1 = self.bottom().max(other.bottom());

        let mut text = String::with_capacity(self.text.len() + other.text.len() + 1);
        text.push_str(&self.text);
        // 中日韩文本词间不加空格；西文需加，否则 "hello" + "world" 会粘成 "helloworld"
        if needs_space_between(&self.text, &other.text) {
            text.push(' ');
        }
        text.push_str(&other.text);

        PixelBox {
            x: x0,
            y: y0,
            w: x1 - x0,
            h: y1 - y0,
            text,
            confidence: combine_confidence(self.confidence, other.confidence),
        }
    }
}

/// 判断两段文本相接是否需要补空格。
///
/// 规则：两侧都是「非 CJK 且非空白」的字符时才补。CJK 之间不补空格，
/// 否则会把「中文」拆成「中 文」。
fn needs_space_between(left: &str, right: &str) -> bool {
    let Some(l) = left.chars().last() else {
        return false;
    };
    let Some(r) = right.chars().next() else {
        return false;
    };
    if l.is_whitespace() || r.is_whitespace() {
        return false;
    }
    !is_cjk(l) && !is_cjk(r)
}

fn is_cjk(c: char) -> bool {
    matches!(c as u32,
        0x3000..=0x303F      // CJK 标点
        | 0x3040..=0x30FF    // 平假名/片假名
        | 0x3400..=0x4DBF    // 扩展 A
        | 0x4E00..=0x9FFF    // 基本区
        | 0xAC00..=0xD7AF    // 谚文音节
        | 0xF900..=0xFAFF    // 兼容表意
        | 0xFF00..=0xFFEF) // 全角
}

/// 置信度合并取较保守值（较小的那个），避免一个高置信词拉高整段。
fn combine_confidence(a: Option<f32>, b: Option<f32>) -> Option<f32> {
    match (a, b) {
        (Some(x), Some(y)) => Some(x.min(y)),
        (Some(x), None) | (None, Some(x)) => Some(x),
        (None, None) => None,
    }
}

/// 把词级框按几何聚合成行。
///
/// 判据：两词**垂直中心**之差不超过 `y_tolerance` 即视为同行；
/// 同行的词按 x 排序后依次合并。**不按输入顺序**分组——OCR 引擎的
/// 输出顺序在不同版本/平台间不保证，按顺序分组会在多栏排版下错行。
pub fn merge_words_into_lines(words: &[PixelBox], y_tolerance: u32) -> Vec<PixelBox> {
    if words.is_empty() {
        return Vec::new();
    }

    // 按垂直中心排序，使同一行的词相邻
    let mut sorted: Vec<&PixelBox> = words.iter().collect();
    sorted.sort_by_key(|w| (w.center_y(), w.x));

    // 分两步：先按垂直邻近分组，再在组内按 x 排序后合并。
    //
    // ⚠️ 不能直接按 `(center_y, x)` 的顺序逐词合并：同一行的词垂直中心会因
    // 字体基线/下伸部（如 g、y）差异错开 1–2px，而 `center_y` 是排序首要键，
    // 于是 "Hello world again" 会被重排成 "Hello again world"——译文语序错乱。
    // 分组判据（与当前已合并行的中心比较）与原先逐词合并**完全一致**，
    // 故不改变既有分行行为，只修正行内次序。
    let mut groups: Vec<Vec<&PixelBox>> = Vec::new();
    let mut merged: Vec<PixelBox> = Vec::new();
    for word in sorted {
        match merged.last_mut() {
            Some(line) if word.center_y().abs_diff(line.center_y()) <= y_tolerance => {
                *line = line.merge_with(word);
                if let Some(group) = groups.last_mut() {
                    group.push(word);
                }
            }
            _ => {
                merged.push(word.clone());
                groups.push(vec![word]);
            }
        }
    }

    let mut lines: Vec<PixelBox> = Vec::new();
    for mut group in groups {
        group.sort_by_key(|w| w.x);
        let mut iter = group.into_iter();
        let Some(first) = iter.next() else { continue };
        let mut current_line = first.clone();
        for w in iter {
            let max_gap =
                ((current_line.h.max(w.h) as f32) * LINE_MERGE_MAX_X_GAP_RATIO).round() as u32;
            let gap = w.x.saturating_sub(current_line.right());
            if gap <= max_gap {
                current_line = current_line.merge_with(w);
            } else {
                lines.push(current_line);
                current_line = w.clone();
            }
        }
        lines.push(current_line);
    }

    lines.sort_by_key(|l| (l.y, l.x));
    lines
}

/// 判定两行是否属于同一段落。
///
/// 需同时满足：
/// - 垂直间距不超过 `max_gap_px`（断开太远视为不同段）；
/// - 水平重叠比例不低于 `min_x_overlap`（左右两栏即使间距很近也**不得**并段）。
///
/// 水平重叠判据是分栏排版的关键：两栏文本的垂直间距往往为 0（同一行高度），
/// 只靠垂直间距必然把左右两栏并成一段，译文会串行错乱。
fn should_merge_lines(
    upper: &PixelBox,
    lower: &PixelBox,
    max_gap_px: u32,
    min_x_overlap: f32,
) -> bool {
    // 垂直间距：下行的顶到上行的底
    let gap = lower.y.saturating_sub(upper.bottom());
    if gap > max_gap_px {
        return false;
    }

    // 水平重叠：交集宽度 / 较窄者的宽度
    let overlap_start = upper.x.max(lower.x);
    let overlap_end = upper.right().min(lower.right());
    let overlap = overlap_end.saturating_sub(overlap_start) as f32;
    let narrower = upper.w.min(lower.w);
    if narrower == 0 {
        return false;
    }
    let ratio = overlap / narrower as f32;
    ratio >= min_x_overlap
}

/// 把行聚合成段落。
///
/// 输入必须是已按 `(y, x)` 排序的行（[`merge_words_into_lines`] 的输出）。
///
/// # 为什么必须与**所有**已有段落比较，而非只比最后一个
///
/// 行按 `(y, x)` 排序后，分栏排版的行序是「左1、右1、左2、右2」。
/// 若只与最后一个段落比较：左2 会去比右1（水平不重叠）→ 另起一段，
/// 于是每一行都各自成段——既丢上下文（翻译质量下降），又让请求数翻倍。
/// 必须回扫已有段落，把左2 并回左栏那一段。
pub fn merge_lines_into_paragraphs(
    lines: &[PixelBox],
    max_gap_px: u32,
    min_x_overlap: f32,
) -> Vec<PixelBox> {
    if lines.is_empty() {
        return Vec::new();
    }

    let mut paragraphs: Vec<PixelBox> = Vec::new();
    for line in lines {
        // 从最新段落往前找第一个几何相容者（最近优先，贴合文档顺序）
        let target = paragraphs
            .iter_mut()
            .rev()
            .find(|p| should_merge_lines(p, line, max_gap_px, min_x_overlap));

        match target {
            Some(para) => *para = para.merge_with(line),
            None => paragraphs.push(line.clone()),
        }
    }

    // 合并过程中可能改变垂直范围，按位置重排以保证输出顺序稳定
    paragraphs.sort_by_key(|p| (p.y, p.x));
    paragraphs
}

/// 送往前端的 OCR 文字块。
///
/// 坐标是**相对喂给 OCR 那张图**的归一化值（原点左上）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct OcrBlockDto {
    /// 稳定序号（与请求数组下标一致，供前端按 id 回填译文）
    pub index: u32,
    pub text: String,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    /// 段落内行数（前端据此估算行高与字号）
    pub line_count: u32,
    /// 置信度 ∈ [0,1]；引擎不提供时为 `None`
    pub confidence: Option<f32>,
}

/// 低于此置信度的块直接丢弃（0–1 尺度）。
///
/// tesseract 的 `conf` 是 0–100 整数，调用方需先除以 100；
/// `conf < 0` 表示该行无有效识别结果，应在解析阶段就丢弃。
pub const MIN_BLOCK_CONFIDENCE: f32 = 0.5;

/// 行聚合的垂直容差：平均词高的这一倍。
///
/// 同一行的词因字体基线/下伸部（如 g、y）差异，垂直中心会小幅错开。
/// 取平均词高的 0.6 倍：足以吸收该差异，又小于行距（通常 ≥ 1.2 倍词高），
/// 不会把相邻两行并成一行。
pub const LINE_MERGE_Y_TOLERANCE_RATIO: f32 = 0.6;

/// 行聚合的最大水平词间距：词高的这一倍。
///
/// 正常排版下西文词距约 0.3–0.6 倍字高，制表符或宽松排版不超过 2 倍；
/// 超过该阈值（如两栏文本、左右对齐控件、表格单元格）不得并入同一行，
/// 必须拆分为独立行片段，避免将左右两栏横向串联成一行。
pub const LINE_MERGE_MAX_X_GAP_RATIO: f32 = 2.5;

/// 段聚合的最大垂直间距：平均词高的这一倍。
///
/// 段落内的行距通常小于 1 倍词高；超过则视为另起一段（如标题与正文之间）。
pub const PARAGRAPH_MAX_GAP_RATIO: f32 = 1.5;

/// 段聚合所需的最小水平重叠比例。
///
/// 分栏排版的关键：左右两栏即使垂直间距为 0，也不得并段。
pub const PARAGRAPH_MIN_X_OVERLAP: f32 = 0.3;

/// 把平台 OCR 产出的**词级**框聚合成送往前端的归一化块。
///
/// 这是三平台共用的收口点：平台层只负责「拿到词」，
/// 聚合与归一化是纯逻辑，必须留在 core 才能获得测试覆盖
/// （平台代码在非目标平台上会被 `cfg` 剔除）。
///
/// # 参数
///
/// - `words`：词级框，坐标是**喂给 OCR 那张图**的像素；
/// - `img_w` / `img_h`：**同一张图**的像素尺寸。
///
/// ⚠️ 若 OCR 前裁剪过选区，`img_w`/`img_h` 必须是**裁剪后**的尺寸，
/// 不是原图尺寸。用错分母会让所有块整体偏移。
///
/// 返回的块已过 [`filter_blocks`]，故 `index` 与数组下标严格一致。
pub fn words_to_blocks(words: &[PixelBox], img_w: u32, img_h: u32) -> Vec<OcrBlockDto> {
    if words.is_empty() || img_w == 0 || img_h == 0 {
        return Vec::new();
    }

    // 聚合容差按「平均词高」自适应：固定像素值在 4K 与低分屏之间不可通用。
    // 用词高而非字高，是因为词框已包含该词的实际字体高度。
    let mean_height = {
        let sum: u64 = words.iter().map(|w| u64::from(w.h)).sum();
        (sum / words.len() as u64).max(1) as u32
    };
    let y_tolerance = (mean_height as f32 * LINE_MERGE_Y_TOLERANCE_RATIO).round() as u32;
    let max_gap = (mean_height as f32 * PARAGRAPH_MAX_GAP_RATIO).round() as u32;

    let lines = merge_words_into_lines(words, y_tolerance);
    let paragraphs = merge_lines_into_paragraphs(&lines, max_gap, PARAGRAPH_MIN_X_OVERLAP);

    let blocks: Vec<OcrBlockDto> = paragraphs
        .into_iter()
        .map(|p| {
            let rect = NormRect::from_pixels_in(p.x, p.y, p.w, p.h, img_w, img_h);
            OcrBlockDto {
                // 占位；filter_blocks 会重排为与下标一致
                index: 0,
                text: p.text,
                x: rect.x,
                y: rect.y,
                w: rect.w,
                h: rect.h,
                // 段落是行合并的产物，行数信息在合并时丢失；按高度反推。
                // 前端只用它估行高与字号，不参与坐标计算，近似即可。
                line_count: (p.h as f32 / mean_height as f32).round().max(1.0) as u32,
                confidence: p.confidence,
            }
        })
        .collect();

    filter_blocks(blocks)
}

/// 过滤不可用的 OCR 块。
///
/// 过滤项：空白文本、过短文本、低置信度。
/// **不过滤会让重复译文叠印在同一位置**，或把噪点识别结果也送去翻译。
pub fn filter_blocks(blocks: Vec<OcrBlockDto>) -> Vec<OcrBlockDto> {
    let mut out: Vec<OcrBlockDto> = blocks
        .into_iter()
        .filter(|b| {
            let trimmed = b.text.trim();
            // 单字符块通常是噪点或标点碎片，但 CJK 单字是完整的语义独立词（如「门」「开」「停」），不得误杀
            if trimmed.chars().count() < 2 && !trimmed.chars().any(is_cjk) {
                return false;
            }
            // 纯符号/数字块无翻译价值（如页码、分隔线）
            if !trimmed.chars().any(|c| c.is_alphabetic() || is_cjk(c)) {
                return false;
            }
            if let Some(conf) = b.confidence {
                if conf < MIN_BLOCK_CONFIDENCE {
                    return false;
                }
            }
            // 退化为零尺寸的块无法渲染
            b.w > 0.0 && b.h > 0.0
        })
        .collect();

    // 过滤后重新编号，保证 index 与数组下标严格一致
    for (i, b) in out.iter_mut().enumerate() {
        b.index = i as u32;
    }
    out
}

/// 把块按 [`crate::MAX_BATCH_ITEMS`] 与 [`crate::MAX_BATCH_TOTAL_CHARS`] 切成连续区间。
///
/// 直接复用 `plan_batch_ranges`，保证与扩展端、服务端**共用同一份上限契约**。
pub fn plan_block_batches(blocks: &[OcrBlockDto]) -> Vec<(usize, usize)> {
    crate::engine::plan_batch_ranges(blocks.iter().map(|b| b.text.as_str()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn word(x: u32, y: u32, t: &str) -> PixelBox {
        // 统一用 10px 高的词框，便于推演行聚合
        PixelBox::new(x, y, 40, 10, t)
    }

    #[test]
    fn test_merge_words_single_line() {
        let words = vec![word(0, 0, "Hello"), word(45, 1, "world")];
        let lines = merge_words_into_lines(&words, 5);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].text, "Hello world");
        assert_eq!(lines[0].x, 0);
        assert_eq!(lines[0].right(), 85);
    }

    /// 关键回归：必须按几何聚类，而非按输入顺序。
    ///
    /// 输入顺序被打乱（第二行的词排在前面）时，按顺序分组的实现会产出乱序文本。
    #[test]
    fn test_merge_words_clusters_by_geometry_not_input_order() {
        let words = vec![
            // 第二行的词故意排在前面
            word(0, 100, "second"),
            word(0, 0, "first"),
            word(45, 100, "line"),
            word(45, 0, "line"),
        ];
        let lines = merge_words_into_lines(&words, 5);
        assert_eq!(lines.len(), 2);
        // 按 y 排序后，第一行在前
        assert_eq!(lines[0].text, "first line");
        assert_eq!(lines[1].text, "second line");
    }

    #[test]
    fn test_merge_words_y_tolerance_boundary() {
        // 中心差恰好等于容差 → 同行
        let same = vec![word(0, 0, "a"), word(50, 10, "b")]; // center 5 vs 15，差 10
        assert_eq!(merge_words_into_lines(&same, 10).len(), 1);
        // 中心差超出容差 → 分行
        let diff = vec![word(0, 0, "a"), word(50, 11, "b")]; // 差 11
        assert_eq!(merge_words_into_lines(&diff, 10).len(), 2);
    }

    /// CJK 之间不补空格，西文之间补。
    #[test]
    fn test_merge_words_space_insertion_by_script() {
        let cjk = merge_words_into_lines(&[word(0, 0, "中文"), word(45, 0, "翻译")], 5);
        assert_eq!(cjk[0].text, "中文翻译", "CJK 之间不应插入空格");

        let latin = merge_words_into_lines(&[word(0, 0, "hello"), word(45, 0, "world")], 5);
        assert_eq!(latin[0].text, "hello world");

        // 中英混排：一侧是 CJK 就不补空格
        let mixed = merge_words_into_lines(&[word(0, 0, "中文"), word(45, 0, "English")], 5);
        assert_eq!(mixed[0].text, "中文English");
    }

    #[test]
    fn test_merge_words_confidence_takes_minimum() {
        let mut a = word(0, 0, "a");
        a.confidence = Some(0.9);
        let mut b = word(45, 0, "b");
        b.confidence = Some(0.6);
        let lines = merge_words_into_lines(&[a, b], 5);
        assert_eq!(lines[0].confidence, Some(0.6));

        // 一侧无置信度则取另一侧（而非丢弃）
        let mut c = word(0, 0, "c");
        c.confidence = None;
        let mut d = word(45, 0, "d");
        d.confidence = Some(0.7);
        let lines2 = merge_words_into_lines(&[c, d], 5);
        assert_eq!(lines2[0].confidence, Some(0.7));
    }

    #[test]
    fn test_merge_words_empty() {
        assert!(merge_words_into_lines(&[], 5).is_empty());
    }

    /// 关键回归：两栏排版不得并成一段。
    ///
    /// 左右两栏的垂直间距为 0（同一行高度），只靠垂直间距的实现必然误并，
    /// 译文会串行错乱。
    #[test]
    fn test_merge_lines_does_not_merge_columns() {
        // 左栏 x=0..100，右栏 x=300..400，两行都在 y=0 与 y=15
        let lines = vec![
            PixelBox::new(0, 0, 100, 12, "left column line1"),
            PixelBox::new(300, 0, 100, 12, "right column line1"),
            PixelBox::new(0, 15, 100, 12, "left column line2"),
            PixelBox::new(300, 15, 100, 12, "right column line2"),
        ];
        let paras = merge_lines_into_paragraphs(&lines, 100, 0.5);
        // 应先分成两行（左/右），再各自向下聚合
        let left = paras.iter().find(|p| p.text.contains("left column line1"));
        let right = paras.iter().find(|p| p.text.contains("right column line1"));
        assert!(left.is_some() && right.is_some());

        let left = left.unwrap();
        assert!(
            !left.text.contains("right column"),
            "左右两栏被误并为一段：{}",
            left.text
        );
        assert!(left.text.contains("left column line2"), "同栏的两行应合并");
    }

    #[test]
    fn test_merge_lines_respects_max_gap() {
        let lines = vec![
            PixelBox::new(0, 0, 100, 12, "para1"),
            PixelBox::new(0, 14, 100, 12, "same para"), // gap = 2
            PixelBox::new(0, 60, 100, 12, "para2"),     // gap = 34
        ];
        let paras = merge_lines_into_paragraphs(&lines, 10, 0.5);
        assert_eq!(paras.len(), 2, "间距超限应新起一段");
        assert!(paras[0].text.contains("same para"));
        assert_eq!(paras[1].text, "para2");
    }

    #[test]
    fn test_merge_lines_overlap_ratio_boundary() {
        // 上行 0..100，下行 0..50：交集 50，较窄者 50 → ratio = 1.0
        let lines = vec![
            PixelBox::new(0, 0, 100, 12, "upper"),
            PixelBox::new(0, 14, 50, 12, "lower"),
        ];
        assert_eq!(merge_lines_into_paragraphs(&lines, 10, 0.5).len(), 1);
        // 阈值提到 1.01 则不应合并
        assert_eq!(merge_lines_into_paragraphs(&lines, 10, 1.01).len(), 2);

        // 完全错开（交集 0）→ 不合并
        let no_overlap = vec![
            PixelBox::new(0, 0, 100, 12, "upper"),
            PixelBox::new(200, 14, 100, 12, "lower"),
        ];
        assert_eq!(merge_lines_into_paragraphs(&no_overlap, 10, 0.1).len(), 2);
    }

    #[test]
    fn test_filter_blocks_drops_unusable() {
        let mk = |text: &str, conf: Option<f32>, w: f32| OcrBlockDto {
            index: 0,
            text: text.to_string(),
            x: 0.1,
            y: 0.1,
            w,
            h: 0.05,
            line_count: 1,
            confidence: conf,
        };

        let blocks = vec![
            mk("valid text", Some(0.9), 0.2), // 保留
            mk("   ", Some(0.9), 0.2),        // 空白 → 丢
            mk("a", Some(0.9), 0.2),          // 西文单字符噪点 → 丢
            mk("中", Some(0.9), 0.2),         // CJK 单字为完整语义词 → 保留
            mk("12345", Some(0.9), 0.2),      // 纯数字 → 丢
            mk("--- ===", Some(0.9), 0.2),    // 纯符号 → 丢
            mk("low conf", Some(0.3), 0.2),   // 低置信度 → 丢
            mk("zero width", Some(0.9), 0.0), // 零宽 → 丢
            mk("无置信度", None, 0.2),        // 无置信度 → 保留
        ];
        let out = filter_blocks(blocks);
        assert_eq!(
            out.len(),
            3,
            "实际保留：{:?}",
            out.iter().map(|b| &b.text).collect::<Vec<_>>()
        );
        assert_eq!(out[0].text, "valid text");
        assert_eq!(out[1].text, "中");
        assert_eq!(out[2].text, "无置信度");
        // 重新编号
        assert_eq!(out[0].index, 0);
        assert_eq!(out[1].index, 1);
        assert_eq!(out[2].index, 2);
    }

    /// 关键回归：切批必须同时满足项数与字符量两个上限。
    #[test]
    fn test_plan_block_batches_respects_both_limits() {
        let mk = |text: String| OcrBlockDto {
            index: 0,
            text,
            x: 0.0,
            y: 0.0,
            w: 0.1,
            h: 0.1,
            line_count: 1,
            confidence: None,
        };

        // 项数上限：40 个短块 → 至少切成 2 批
        let many: Vec<OcrBlockDto> = (0..40).map(|_| mk("short".to_string())).collect();
        let ranges = plan_block_batches(&many);
        assert!(ranges.len() >= 2, "40 项超过 MAX_BATCH_ITEMS=30 却未切批");
        for (s, e) in &ranges {
            assert!(e - s <= crate::MAX_BATCH_ITEMS, "区间项数越界：{}", e - s);
        }
        // 覆盖且不重叠
        assert_eq!(ranges.first().unwrap().0, 0);
        assert_eq!(ranges.last().unwrap().1, many.len());
        for w in ranges.windows(2) {
            assert_eq!(w[0].1, w[1].0, "区间不连续");
        }

        // 字符量上限：6 个 10000 字的块 → 总量 60000 超 50000，必须切批。
        // 每块必须 ≤ MAX_ITEM_CHARS：否则会先被「单项超限独占」规则隔开，
        // 测到的就不再是字符总量上限（原本用 25000 字块的版本正因此失效）。
        let long: Vec<OcrBlockDto> = (0..6).map(|_| mk("字".repeat(10_000))).collect();
        let ranges2 = plan_block_batches(&long);
        assert_eq!(
            ranges2,
            vec![(0, 5), (5, 6)],
            "字符量越界却未切批：{ranges2:?}"
        );
        for (s, e) in &ranges2 {
            let total: usize = long[*s..*e].iter().map(|b| b.text.chars().count()).sum();
            assert!(
                total <= crate::MAX_BATCH_TOTAL_CHARS,
                "区间字符量越界：{total}"
            );
        }

        // 单项即超限（>50000 字）时不静默丢弃，应独占一个区间
        let huge = vec![mk("字".repeat(60_000)), mk("short".to_string())];
        let ranges3 = plan_block_batches(&huge);
        assert_eq!(ranges3, vec![(0, 1), (1, 2)], "超限单项应独占区间");
    }

    #[test]
    fn test_plan_block_batches_empty() {
        assert!(plan_block_batches(&[]).is_empty());
    }

    #[test]
    fn test_is_cjk_coverage() {
        assert!(is_cjk('中'));
        assert!(is_cjk('あ'));
        assert!(is_cjk('한'));
        assert!(is_cjk('，'));
        assert!(!is_cjk('a'));
        assert!(!is_cjk('1'));
        assert!(!is_cjk(' '));
    }

    /// 关键回归：坐标必须按「喂给 OCR 那张图」的尺寸归一化。
    ///
    /// 两处独立缺陷都靠这条捕获：① 坐标轴搞反（x/y 互换）；
    /// ② 归一化分母用错（用原图尺寸而非裁剪图尺寸）。
    #[test]
    fn test_words_to_blocks_normalizes_into_correct_quadrants() {
        // 1000x500 的图，左上角一个词、右下角一个词
        let words = vec![
            PixelBox::new(50, 40, 100, 20, "Hello"),
            PixelBox::new(800, 400, 150, 20, "世界"),
        ];
        let blocks = words_to_blocks(&words, 1000, 500);
        assert_eq!(blocks.len(), 2);

        let hello = blocks.iter().find(|b| b.text == "Hello").expect("Hello 块");
        assert!(
            hello.x < 0.3 && hello.y < 0.3,
            "Hello 应落在左上: {hello:?}"
        );

        let world = blocks.iter().find(|b| b.text == "世界").expect("世界 块");
        assert!(world.x > 0.6 && world.y > 0.6, "世界 应落在右下: {world:?}");

        // 精确值：x = 800/1000 = 0.8，y = 400/500 = 0.8
        assert!((world.x - 0.8).abs() < 1e-6, "x 应精确为 0.8: {}", world.x);
        assert!((world.y - 0.8).abs() < 1e-6, "y 应精确为 0.8: {}", world.y);
    }

    /// 关键回归：裁剪后的子图尺寸才是归一化分母。
    ///
    /// 同一个词在「全图 1000x500」与「裁剪图 400x200」下归一化结果必须不同；
    /// 用错分母会让所有块整体偏移（本例 0.8 vs 0.2）。
    #[test]
    fn test_words_to_blocks_uses_downsampled_or_cropped_denominator() {
        let words = vec![PixelBox::new(320, 160, 40, 20, "cropped")];

        let in_full = words_to_blocks(&words, 1000, 500);
        let in_crop = words_to_blocks(&words, 400, 200);
        assert_eq!(in_full.len(), 1);
        assert_eq!(in_crop.len(), 1);

        assert!(
            (in_full[0].x - 0.32).abs() < 1e-6,
            "全图分母: {}",
            in_full[0].x
        );
        assert!(
            (in_crop[0].x - 0.8).abs() < 1e-6,
            "裁剪分母: {}",
            in_crop[0].x
        );
        assert!(
            (in_full[0].x - in_crop[0].x).abs() > 0.1,
            "两种分母必须给出不同结果，否则分母没起作用"
        );
    }

    /// 关键回归：同一行的多个词必须合成一块，而不是逐词成块。
    ///
    /// 逐词送翻译会因上下文缺失导致语序崩坏（与 LESSONS 第 8 条同源）。
    #[test]
    fn test_words_to_blocks_merges_same_line_words() {
        let words = vec![
            PixelBox::new(10, 10, 60, 20, "Hello"),
            PixelBox::new(80, 12, 70, 20, "world"),
            PixelBox::new(160, 10, 40, 20, "again"),
        ];
        let blocks = words_to_blocks(&words, 1000, 500);
        assert_eq!(blocks.len(), 1, "同一行应合成 1 块: {blocks:?}");
        assert_eq!(blocks[0].text, "Hello world again");
    }

    /// 相邻两行且水平重叠 → 必须并成同一段（保留上下文）。
    #[test]
    fn test_words_to_blocks_merges_adjacent_lines_into_paragraph() {
        let words = vec![
            PixelBox::new(10, 10, 100, 20, "first"),
            PixelBox::new(10, 34, 120, 20, "second"),
        ];
        let blocks = words_to_blocks(&words, 1000, 500);
        assert_eq!(blocks.len(), 1, "相邻两行应并段: {blocks:?}");
        assert!(blocks[0].text.contains("first") && blocks[0].text.contains("second"));
    }

    /// 退化输入：空词表、零尺寸图都不得 panic，返回空块。
    #[test]
    fn test_words_to_blocks_degenerate_inputs() {
        assert!(words_to_blocks(&[], 1000, 500).is_empty());
        assert!(words_to_blocks(&[PixelBox::new(1, 1, 10, 10, "x")], 0, 0).is_empty());
    }

    /// 关键回归：低置信度词必须被过滤掉（否则噪点也会被送去翻译）。
    #[test]
    fn test_words_to_blocks_filters_low_confidence() {
        let mut low = PixelBox::new(10, 10, 100, 20, "noise");
        low.confidence = Some(0.1);
        let mut high = PixelBox::new(10, 100, 100, 20, "valid");
        high.confidence = Some(0.9);

        let blocks = words_to_blocks(&[low, high], 1000, 500);
        assert_eq!(blocks.len(), 1, "低置信度块应被过滤: {blocks:?}");
        assert_eq!(blocks[0].text, "valid");
    }

    /// 关键回归：多栏排版在同一水平高度时不应横向合并成一行，而应保留分栏段落。
    #[test]
    fn test_words_to_blocks_splits_multiple_columns() {
        let words = vec![
            // 左栏第 1 行与右栏第 1 行（都在 y=10）
            PixelBox::new(10, 10, 40, 20, "left_a"),
            PixelBox::new(55, 10, 40, 20, "left_b"),
            PixelBox::new(300, 10, 40, 20, "right_a"),
            PixelBox::new(345, 10, 40, 20, "right_b"),
            // 左栏第 2 行与右栏第 2 行（都在 y=35）
            PixelBox::new(10, 35, 40, 20, "left_c"),
            PixelBox::new(55, 35, 40, 20, "left_d"),
            PixelBox::new(300, 35, 40, 20, "right_c"),
            PixelBox::new(345, 35, 40, 20, "right_d"),
        ];
        let blocks = words_to_blocks(&words, 1000, 500);
        assert_eq!(
            blocks.len(),
            2,
            "左右两栏应聚合成 2 个独立段落块: {blocks:?}"
        );
        let left = blocks
            .iter()
            .find(|b| b.text.contains("left_a"))
            .expect("左栏块");
        let right = blocks
            .iter()
            .find(|b| b.text.contains("right_a"))
            .expect("右栏块");
        assert!(
            !left.text.contains("right"),
            "左栏不应包含右栏文本: {}",
            left.text
        );
        assert!(
            !right.text.contains("left"),
            "右栏不应包含左栏文本: {}",
            right.text
        );
        assert!(
            left.text.contains("left_c"),
            "左栏两行应合并: {}",
            left.text
        );
        assert!(
            right.text.contains("right_c"),
            "右栏两行应合并: {}",
            right.text
        );
    }

    /// 关键回归：单个 CJK 汉字是完整的语义独立词，不可被 filter_blocks 丢弃。
    #[test]
    fn test_words_to_blocks_preserves_single_cjk_characters() {
        let words = vec![
            PixelBox::new(10, 10, 30, 30, "门"),
            PixelBox::new(10, 100, 30, 30, "开"),
        ];
        let blocks = words_to_blocks(&words, 1000, 500);
        assert_eq!(blocks.len(), 2, "单字汉字均应保留成块: {blocks:?}");
        assert_eq!(blocks[0].text, "门");
        assert_eq!(blocks[1].text, "开");
    }

    /// 过滤后 index 必须与数组下标严格一致（前端按 index 回填译文）。
    #[test]
    fn test_words_to_blocks_reindexes_after_filter() {
        let mut noise = PixelBox::new(10, 10, 100, 20, "!");
        noise.confidence = Some(0.1);
        let words = vec![
            noise,
            PixelBox::new(10, 100, 100, 20, "alpha"),
            PixelBox::new(10, 200, 100, 20, "beta"),
        ];
        let blocks = words_to_blocks(&words, 1000, 500);
        for (i, b) in blocks.iter().enumerate() {
            assert_eq!(b.index, i as u32, "index 必须与下标一致: {blocks:?}");
        }
    }
}

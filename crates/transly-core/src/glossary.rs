use aho_corasick::{AhoCorasick, MatchKind};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};

#[derive(Debug, Clone)]
struct TermMeta {
    source: String,
    target: String,
    match_case: bool,
    word_boundary: bool,
}

/// 预编译的匹配器：AC 自动机 + 与 pattern 下标对齐的确定性元数据表。
#[derive(Debug)]
struct Compiled {
    ac: AhoCorasick,
    metas: Vec<TermMeta>,
}

#[derive(Debug, Default)]
pub struct Glossary {
    terms: HashMap<String, TermMeta>,
    /// 词条集合变更后置位；`apply` 重建后复位，避免每次调用都重编自动机。
    dirty: AtomicBool,
    compiled: RwLock<Option<Arc<Compiled>>>,
}

impl Clone for Glossary {
    fn clone(&self) -> Self {
        // 克隆只携带词条，预编译缓存整体丢弃（调用方通常随即失效重建）。
        Self {
            terms: self.terms.clone(),
            dirty: AtomicBool::new(true),
            compiled: RwLock::new(None),
        }
    }
}

/// 判定字符是否构成"词内字符"。
///
/// 对于西文单词边界校验，中日韩（CJK）文字天然构成单词边界，
/// 不得将 CJK 字符视作西文词内字符（`c.is_alphanumeric()` 对汉字恒为 true），
/// 否则「用Rust重构」中的「Rust」会被前后汉字拒绝替换。
#[inline]
fn is_word_char(c: char) -> bool {
    (c.is_alphanumeric() && !is_cjk(c)) || c == '_'
}

/// 该词条所属书写系统是否存在词边界概念。
///
/// 中日韩书写系统不使用空格分词，「机器」与「机器学习」在字符层面完全相邻，
/// 无法靠前后字符区分。对这类语言做字符级边界判定会让词条彻底无法命中
/// （「机器学习和机器视觉」中两个「机器」都会被判为词内而拒绝）。
///
/// 术语库的既定语义是「全局强制优先精确匹配」（见 README），因此对无词边界
/// 的书写系统一律不做边界判定——需要精确命中的复合术语应由用户录入完整词条。
///
/// 而拉丁字母扩展（café / naïve）、西里尔、希腊等书写系统有明确词边界，
/// 必须参与边界判定，否则会腰斩出「a 咖啡teria」这样的破损译文。
fn has_word_boundaries(src: &str) -> bool {
    !src.chars().any(is_cjk)
}

#[inline]
fn is_cjk(c: char) -> bool {
    matches!(c as u32,
        0x3040..=0x30FF        // 平假名 / 片假名
        | 0x3400..=0x4DBF      // 汉字扩展 A
        | 0x4E00..=0x9FFF      // 汉字基本区
        | 0xF900..=0xFAFF      // 汉字兼容表意文字
        | 0xAC00..=0xD7A3      // 谚文音节
    )
}

impl Glossary {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, source: impl Into<String>, target: impl Into<String>) {
        self.insert_with_options(source, target, true);
    }

    pub fn insert_with_options(
        &mut self,
        source: impl Into<String>,
        target: impl Into<String>,
        match_case: bool,
    ) {
        let src = source.into();
        let tgt = target.into();

        if src.trim().is_empty() {
            return;
        }

        let word_boundary = has_word_boundaries(&src)
            && src.chars().next().map(is_word_char).unwrap_or(false)
            && src.chars().next_back().map(is_word_char).unwrap_or(false);

        self.terms.insert(
            src.clone(),
            TermMeta {
                source: src,
                target: tgt,
                match_case,
                word_boundary,
            },
        );
        self.dirty.store(true, Ordering::Relaxed);
    }

    /// 词条集合未变更时复用已编译自动机；否则重建并回填缓存。
    fn ensure_compiled(&self) -> Option<Arc<Compiled>> {
        if !self.dirty.load(Ordering::Relaxed) {
            if let Ok(guard) = self.compiled.read() {
                if let Some(c) = guard.as_ref() {
                    return Some(Arc::clone(c));
                }
            }
        }

        let built = Arc::new(self.build_compiled()?);
        if let Ok(mut guard) = self.compiled.write() {
            *guard = Some(Arc::clone(&built));
        }
        self.dirty.store(false, Ordering::Relaxed);
        Some(built)
    }

    fn build_compiled(&self) -> Option<Compiled> {
        // 确定性排序：按模式长度降序，同长按字典序升序（长词优先策略）
        let mut metas: Vec<TermMeta> = self.terms.values().cloned().collect();
        metas.sort_by(|a, b| {
            b.source
                .len()
                .cmp(&a.source.len())
                .then_with(|| a.source.cmp(&b.source))
        });

        let patterns: Vec<&str> = metas.iter().map(|m| m.source.as_str()).collect();

        // 采用 Standard 模式构建支持大小写折叠的自动机，配合 find_overlapping_iter
        // 提取全部重叠候选区间，并在业务层后验过滤 match_case 与 word_boundary。
        // 随后在候选池中执行贪婪 Leftmost-Longest 匹配，杜绝 LeftmostLongest
        // 内部提前剪枝导致同形大小写敏感词或子词被静默吞掉的缺陷。
        let ac = AhoCorasick::builder()
            .match_kind(MatchKind::Standard)
            .ascii_case_insensitive(true)
            .build(&patterns)
            .ok()?;

        Some(Compiled { ac, metas })
    }

    pub fn apply(&self, text: &str) -> String {
        if self.terms.is_empty() || text.is_empty() {
            return text.to_string();
        }

        let compiled = match self.ensure_compiled() {
            Some(c) => c,
            None => return text.to_string(),
        };
        let Compiled { ac, metas } = &*compiled;

        struct CandidateMatch<'a> {
            start: usize,
            end: usize,
            pattern_idx: usize,
            target: &'a str,
        }

        let mut valid_matches = Vec::new();

        for mat in ac.find_overlapping_iter(text) {
            let pattern_idx = mat.pattern().as_usize();
            let meta = &metas[pattern_idx];
            let matched_slice = &text[mat.start()..mat.end()];

            // 1. 若要求区分大小写，检查是否与原始定义完全匹配
            if meta.match_case && matched_slice != meta.source.as_str() {
                continue;
            }

            // 2. 若要求单词边界，检查匹配区段左前驱与右后继
            if meta.word_boundary {
                let is_left_boundary = if mat.start() == 0 {
                    true
                } else {
                    let prev_char = text[..mat.start()].chars().next_back().unwrap_or(' ');
                    !is_word_char(prev_char)
                };

                let is_right_boundary = if mat.end() >= text.len() {
                    true
                } else {
                    let next_char = text[mat.end()..].chars().next().unwrap_or(' ');
                    !is_word_char(next_char)
                };

                if !is_left_boundary || !is_right_boundary {
                    continue;
                }
            }

            valid_matches.push(CandidateMatch {
                start: mat.start(),
                end: mat.end(),
                pattern_idx,
                target: &meta.target,
            });
        }

        // 确定性贪婪排序：按起始位置升序；同起始位置按长度降序（长词优先）；同长度按模式优先级升序
        valid_matches.sort_by(|a, b| {
            a.start
                .cmp(&b.start)
                .then_with(|| (b.end - b.start).cmp(&(a.end - a.start)))
                .then_with(|| a.pattern_idx.cmp(&b.pattern_idx))
        });

        let mut output = String::with_capacity(text.len());
        let mut last_idx = 0;

        for m in valid_matches {
            if m.start < last_idx {
                // 与先前已应用的长词或左侧词重叠，跳过
                continue;
            }
            output.push_str(&text[last_idx..m.start]);
            output.push_str(m.target);
            last_idx = m.end;
        }

        output.push_str(&text[last_idx..]);
        output
    }

    pub fn is_empty(&self) -> bool {
        self.terms.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_glossary_dollar_sign_literal() {
        let mut glossary = Glossary::new();
        glossary.insert("USD", "$100");

        let text = "Pay in USD now";
        let result = glossary.apply(text);
        assert_eq!(result, "Pay in $100 now");
    }

    #[test]
    fn test_glossary_same_length_deterministic() {
        let mut g1 = Glossary::new();
        g1.insert("cat", "猫");
        g1.insert("dog", "狗");

        let mut g2 = Glossary::new();
        g2.insert("dog", "狗");
        g2.insert("cat", "猫");

        let text = "a cat and a dog";
        assert_eq!(g1.apply(text), g2.apply(text));
    }

    #[test]
    fn test_glossary_apply_order() {
        let mut glossary = Glossary::new();
        glossary.insert("deep", "深的");
        glossary.insert("deep learning", "深度学习");

        let text = "I study deep learning";
        let result = glossary.apply(text);
        // 应该先匹配长词 "deep learning"，而不是把 deep 先替换掉导致损坏
        assert_eq!(result, "I study 深度学习");
    }

    #[test]
    fn test_glossary_case_insensitive() {
        let mut glossary = Glossary::new();
        glossary.insert_with_options("agent", "智能体", false);
        glossary.insert_with_options("NLP", "自然语言处理", false);

        let text = "The autonomous Agent and nlp model";
        let result = glossary.apply(text);
        assert_eq!(result, "The autonomous 智能体 and 自然语言处理 model");
    }

    #[test]
    fn test_glossary_case_sensitive_default() {
        let mut glossary = Glossary::new();
        glossary.insert("Apple", "苹果公司");

        let text = "I eat an apple from Apple";
        let result = glossary.apply(text);
        assert_eq!(result, "I eat an apple from 苹果公司");
    }

    #[test]
    fn test_glossary_cascade_prevention() {
        let mut glossary = Glossary::new();
        // 术语 A 替换为包含 "target" 的译文，术语 B 会把 "target" 替换为 "目标"
        // 循环正则中：若 A 先替换，文本中会出现 target，接着被 B 再次替换！
        // 单趟扫描应保证原文本中仅匹配原始出现的词条，绝不会产生二次级联替换！
        glossary.insert("source", "my target output");
        glossary.insert("target", "目标");

        let text = "this is source text";
        let result = glossary.apply(text);
        assert_eq!(
            result, "this is my target output text",
            "已替换的内容不应被后续术语再次级联替换"
        );
    }

    #[test]
    fn test_glossary_boundary_check() {
        let mut glossary = Glossary::new();
        glossary.insert("cat", "猫");
        glossary.insert("deep learning", "深度学习");

        // 单词内部子串不应匹配，独立词才匹配
        let text = "scatter the cat in bobcat";
        let result = glossary.apply(text);
        assert_eq!(result, "scatter the 猫 in bobcat");

        // 包含空格的复合短语同样受到首尾单词边界保护
        let phrase_text = "nondeep learning and deep learningpro versus real deep learning here";
        assert_eq!(
            glossary.apply(phrase_text),
            "nondeep learning and deep learningpro versus real 深度学习 here"
        );
    }

    #[test]
    fn test_glossary_case_sensitive_and_insensitive_shadowing() {
        let mut glossary = Glossary::new();
        glossary.insert_with_options("Apple", "苹果公司", true);
        glossary.insert_with_options("apple", "苹果水果", true);
        let text = "I eat an apple from Apple";
        let result = glossary.apply(text);
        assert_eq!(result, "I eat an 苹果水果 from 苹果公司");
    }

    /// 回归：中日韩书写系统无词边界，术语按「全局强制」语义生效。
    ///
    /// 早期实现用 `is_ascii_*` 判定词边界，虽然中文词条因此退化为「不做边界
    /// 检查」而碰巧可用，但同样地把有明确词边界的拉丁扩展（café）也一并放过，
    /// 导致「caféteria」被腰斩。正确做法是按书写系统区分，而不是按 ASCII 区分。
    #[test]
    fn test_glossary_cjk_forced_replacement() {
        let mut glossary = Glossary::new();
        glossary.insert("机器", "装置");

        assert_eq!(
            glossary.apply("机器学习和机器视觉"),
            "装置学习和装置视觉",
            "无词边界书写系统按全局强制语义替换"
        );
        assert_eq!(glossary.apply("这台机器很好"), "这台装置很好");
        assert_eq!(glossary.apply("机器。"), "装置。");
    }

    /// 回归：拉丁扩展词（如 café）存在明确词边界，不应在单词内部被腰斩。
    #[test]
    fn test_glossary_non_ascii_latin_boundary() {
        let mut glossary = Glossary::new();
        glossary.insert("café", "咖啡");

        assert_eq!(
            glossary.apply("a caféteria menu"),
            "a caféteria menu",
            "café 不得命中更长单词 caféteria 的内部"
        );
        assert_eq!(glossary.apply("I want a café"), "I want a 咖啡");
        assert_eq!(glossary.apply("the café, and café"), "the 咖啡, and 咖啡");
    }

    /// 预编译缓存不得掩盖后续的词条变更。
    #[test]
    fn test_glossary_cache_invalidated_on_insert() {
        let mut glossary = Glossary::new();
        glossary.insert("alpha", "甲");
        assert_eq!(glossary.apply("alpha beta"), "甲 beta");

        // 复用已编译自动机后再新增词条，新词条必须立即生效
        glossary.insert("beta", "乙");
        assert_eq!(glossary.apply("alpha beta"), "甲 乙");

        // 长词优先策略在缓存命中后依然成立
        glossary.insert("beta gamma", "乙丙");
        assert_eq!(glossary.apply("alpha beta gamma"), "甲 乙丙");
    }

    /// 回归：中英混排语境下，汉字与英文单词相邻时构成有效边界。
    /// 确保类似「用Rust重构」中的「Rust」能被正常替换，而「Crust」与「Rusting」仍受词边界保护。
    #[test]
    fn test_glossary_cjk_english_boundary() {
        let mut glossary = Glossary::new();
        glossary.insert("Rust", "锈语言");
        glossary.insert("AI", "人工智能");

        // 中文紧挨英文（前后无空格）
        assert_eq!(glossary.apply("用Rust重构"), "用锈语言重构");
        assert_eq!(glossary.apply("学Rust语言"), "学锈语言语言");
        assert_eq!(glossary.apply("这个AI模型很强"), "这个人工智能模型很强");

        // 包含英文前后缀的词内部仍受保护不被破坏
        assert_eq!(glossary.apply("吃Crust面包"), "吃Crust面包");
        assert_eq!(glossary.apply("正在Rusting生锈"), "正在Rusting生锈");
    }

    #[test]
    fn test_glossary_clone_resets_compiled_cache() {
        let mut glossary = Glossary::new();
        glossary.insert("cat", "猫");
        assert_eq!(glossary.apply("cat"), "猫");

        let cloned = glossary.clone();
        assert_eq!(cloned.apply("cat"), "猫");

        // 克隆体与原体互不影响
        let mut cloned2 = cloned.clone();
        cloned2.insert("cat", "猫咪");
        assert_eq!(cloned2.apply("cat"), "猫咪");
        assert_eq!(cloned.apply("cat"), "猫");
    }
}

use aho_corasick::{AhoCorasick, MatchKind};
use std::collections::HashMap;

#[derive(Debug, Clone)]
struct TermMeta {
    source: String,
    target: String,
    match_case: bool,
    word_boundary: bool,
}

#[derive(Debug, Default, Clone)]
pub struct Glossary {
    terms: HashMap<String, TermMeta>,
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

        let starts_with_ascii = src
            .chars()
            .next()
            .map(|c| c.is_ascii_alphanumeric() || c == '_')
            .unwrap_or(false);
        let ends_with_ascii = src
            .chars()
            .next_back()
            .map(|c| c.is_ascii_alphanumeric() || c == '_')
            .unwrap_or(false);
        let word_boundary = starts_with_ascii && ends_with_ascii;

        self.terms.insert(
            src.clone(),
            TermMeta {
                source: src,
                target: tgt,
                match_case,
                word_boundary,
            },
        );
    }

    pub fn apply(&self, text: &str) -> String {
        if self.terms.is_empty() || text.is_empty() {
            return text.to_string();
        }

        // 确定性排序：按模式长度降序，同长按字典序升序（长词优先策略）
        let mut metas: Vec<&TermMeta> = self.terms.values().collect();
        metas.sort_by(|a, b| {
            b.source
                .len()
                .cmp(&a.source.len())
                .then_with(|| a.source.cmp(&b.source))
        });

        // 提取模式串供 AhoCorasick 自动机构建
        let patterns: Vec<&str> = metas.iter().map(|m| m.source.as_str()).collect();

        // 采用 Standard 模式构建支持大小写折叠的自动机，配合 find_overlapping_iter
        // 提取全部重叠候选区间，并在业务层后验过滤 match_case 与 word_boundary。
        // 随后在候选池中执行贪婪 Leftmost-Longest 匹配，杜绝 LeftmostLongest
        // 内部提前剪枝导致同形大小写敏感词或子词被静默吞掉的缺陷。
        let ac = match AhoCorasick::builder()
            .match_kind(MatchKind::Standard)
            .ascii_case_insensitive(true)
            .build(&patterns)
        {
            Ok(ac) => ac,
            Err(_) => return text.to_string(),
        };

        struct CandidateMatch<'a> {
            start: usize,
            end: usize,
            pattern_idx: usize,
            target: &'a str,
        }

        let mut valid_matches = Vec::new();

        for mat in ac.find_overlapping_iter(text) {
            let pattern_idx = mat.pattern().as_usize();
            let meta = metas[pattern_idx];
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
                    !prev_char.is_ascii_alphanumeric() && prev_char != '_'
                };

                let is_right_boundary = if mat.end() >= text.len() {
                    true
                } else {
                    let next_char = text[mat.end()..].chars().next().unwrap_or(' ');
                    !next_char.is_ascii_alphanumeric() && next_char != '_'
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
}

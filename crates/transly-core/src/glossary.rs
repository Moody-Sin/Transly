use std::collections::HashMap;

#[derive(Debug, Clone)]
struct TermEntry {
    target: String,
    regex: Option<regex::Regex>,
}

#[derive(Debug, Default, Clone)]
pub struct Glossary {
    terms: HashMap<String, TermEntry>,
}

impl Glossary {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, source: impl Into<String>, target: impl Into<String>) {
        let src = source.into();
        let tgt = target.into();

        if src.trim().is_empty() {
            return;
        }

        let is_ascii_word =
            !src.is_empty() && src.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
        let re = if is_ascii_word {
            let pattern = format!(r"\b{}\b", regex::escape(&src));
            regex::Regex::new(&pattern).ok()
        } else {
            None
        };

        self.terms.insert(
            src,
            TermEntry {
                target: tgt,
                regex: re,
            },
        );
    }

    pub fn get(&self, source: &str) -> Option<&String> {
        self.terms.get(source).map(|entry| &entry.target)
    }

    pub fn apply(&self, text: &str) -> String {
        let mut result = text.to_string();

        // 按照源词长度降序排列，避免短词先替换破坏长复合术语
        let mut sorted_entries: Vec<(&String, &TermEntry)> = self.terms.iter().collect();
        sorted_entries.sort_by_key(|a| std::cmp::Reverse(a.0.len()));

        for (src, entry) in sorted_entries {
            if let Some(ref re) = entry.regex {
                // 使用 regex::NoExpand 确保目标译文中的 $、反斜杠等字符被作为纯字面量替换，不触发正则捕获组展开
                result = re
                    .replace_all(&result, regex::NoExpand(entry.target.as_str()))
                    .to_string();
            } else {
                result = result.replace(src.as_str(), &entry.target);
            }
        }
        result
    }

    pub fn is_empty(&self) -> bool {
        self.terms.is_empty()
    }

    pub fn len(&self) -> usize {
        self.terms.len()
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
    fn test_glossary_apply_order() {
        let mut glossary = Glossary::new();
        glossary.insert("deep", "深的");
        glossary.insert("deep learning", "深度学习");

        let text = "I study deep learning";
        let result = glossary.apply(text);
        // 应该先匹配长词 "deep learning"，而不是把 deep 先替换掉导致损坏
        assert_eq!(result, "I study 深度学习");
    }
}

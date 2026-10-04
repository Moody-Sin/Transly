use std::collections::HashMap;

#[derive(Debug, Default, Clone)]
pub struct Glossary {
    terms: HashMap<String, String>,
}

impl Glossary {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, source: impl Into<String>, target: impl Into<String>) {
        self.terms.insert(source.into(), target.into());
    }

    pub fn get(&self, source: &str) -> Option<&String> {
        self.terms.get(source)
    }

    pub fn apply(&self, text: &str) -> String {
        let mut result = text.to_string();
        for (src, tgt) in &self.terms {
            // 如果源词完全由 ASCII 字母或数字构成，使用单词边界匹配，避免误伤（如 IT 误伤 limit）
            let is_ascii_word =
                !src.is_empty() && src.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
            if is_ascii_word {
                let pattern = format!(r"\b{}\b", regex::escape(src));
                if let Ok(re) = regex::Regex::new(&pattern) {
                    result = re.replace_all(&result, tgt.as_str()).to_string();
                    continue;
                }
            }
            // 包含东亚字符或非纯英文单词则保持直接替换
            result = result.replace(src, tgt);
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
    fn test_glossary_apply() {
        let mut glossary = Glossary::new();
        glossary.insert("LLM", "大语言模型");
        glossary.insert("Rust", "Rust语言");
        glossary.insert("IT", "信息技术");

        let text = "学习 Rust 与 LLM 很有趣，Wait and limit IT department";
        let result = glossary.apply(text);
        assert_eq!(
            result,
            "学习 Rust语言 与 大语言模型 很有趣，Wait and limit 信息技术 department"
        );
        assert_eq!(glossary.len(), 3);
        assert!(!glossary.is_empty());
    }
}

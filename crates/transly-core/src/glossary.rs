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

        let text = "学习 Rust 与 LLM 很有趣";
        let result = glossary.apply(text);
        assert_eq!(result, "学习 Rust语言 与 大语言模型 很有趣");
        assert_eq!(glossary.len(), 2);
        assert!(!glossary.is_empty());
    }
}

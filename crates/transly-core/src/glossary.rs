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
        for (src, entry) in &self.terms {
            if let Some(ref re) = entry.regex {
                result = re.replace_all(&result, entry.target.as_str()).to_string();
            } else {
                result = result.replace(src, &entry.target);
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

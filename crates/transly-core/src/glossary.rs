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
}

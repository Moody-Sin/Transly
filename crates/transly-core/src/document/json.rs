//! JSON 本地化语言包与字典解析与重构
//!
//! 递归提取 JSON 对象与数组中的叶子字符串值进行翻译，
//! 严格保护所有 Key 键名、数值、布尔值及嵌套层级结构 100% 不变。

use super::ParsedDocument;
use serde_json::Value;

/// 依据 RFC 6901 对 JSON Pointer 片段进行转义
fn escape_pointer_segment(seg: &str) -> String {
    seg.replace('~', "~0").replace('/', "~1")
}

/// 解析 JSON 本地化文件，提取待翻译的叶子字符串值并返回重构闭包。
pub fn parse_json(content: &str) -> ParsedDocument {
    if content.trim().is_empty() {
        return (Vec::new(), Box::new(|_| String::new()));
    }

    let parsed: Value = match serde_json::from_str(content) {
        Ok(v) => v,
        Err(_) => {
            // 非法 JSON 降级回退为纯文本解析
            return super::txt::parse_txt(content);
        }
    };

    let mut chunks = Vec::new();
    let mut pointers = Vec::new();

    fn collect_leaf_strings(
        val: &Value,
        current_pointer: String,
        chunks: &mut Vec<String>,
        pointers: &mut Vec<String>,
    ) {
        match val {
            Value::String(s) => {
                chunks.push(s.clone());
                pointers.push(current_pointer);
            }
            Value::Array(arr) => {
                for (idx, item) in arr.iter().enumerate() {
                    let next_ptr = format!("{current_pointer}/{idx}");
                    collect_leaf_strings(item, next_ptr, chunks, pointers);
                }
            }
            Value::Object(map) => {
                for (k, v) in map {
                    let next_ptr = format!("{}/{}", current_pointer, escape_pointer_segment(k));
                    collect_leaf_strings(v, next_ptr, chunks, pointers);
                }
            }
            _ => {}
        }
    }

    collect_leaf_strings(&parsed, String::new(), &mut chunks, &mut pointers);

    let reconstructor = Box::new(move |translated: Vec<String>| {
        let mut root = parsed;

        for (idx, ptr) in pointers.into_iter().enumerate() {
            if let Some(trans_text) = translated.get(idx) {
                if let Some(target_val) = root.pointer_mut(&ptr) {
                    *target_val = Value::String(trans_text.clone());
                }
            }
        }

        serde_json::to_string_pretty(&root).unwrap_or_default()
    });

    (chunks, reconstructor)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_json_preserves_keys_and_types() {
        let json_str = r#"{
  "app": {
    "title": "Welcome",
    "description": "A translation tool"
  },
  "items": [
    "Item one",
    "Item two"
  ],
  "count": 42,
  "enabled": true
}"#;
        let (chunks, recon) = parse_json(json_str);
        assert_eq!(chunks.len(), 4);
        assert!(chunks.contains(&"Welcome".to_string()));
        assert!(chunks.contains(&"A translation tool".to_string()));
        assert!(chunks.contains(&"Item one".to_string()));
        assert!(chunks.contains(&"Item two".to_string()));

        let translated: Vec<String> = chunks
            .iter()
            .map(|c| match c.as_str() {
                "Welcome" => "欢迎".to_string(),
                "A translation tool" => "一款翻译工具".to_string(),
                "Item one" => "第一项".to_string(),
                "Item two" => "第二项".to_string(),
                other => other.to_string(),
            })
            .collect();
        let out = recon(translated);
        assert!(out.contains("\"title\": \"欢迎\""));
        assert!(out.contains("\"description\": \"一款翻译工具\""));
        assert!(out.contains("\"count\": 42"));
        assert!(out.contains("\"enabled\": true"));
        assert!(out.contains("\"第一项\""));
    }
}

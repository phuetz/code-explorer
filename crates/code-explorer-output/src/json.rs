//! Structured JSON output formatter.
//!
//! Produces machine-readable JSON output suitable for piping to `jq` or
//! consumption by other tools. Activated via the `--json` flag.

use crate::traits::{OutputFormatter, TreeItem};

/// JSON output formatter.
pub struct JsonFormatter;

impl JsonFormatter {
    pub fn new() -> Self {
        Self
    }
}

impl Default for JsonFormatter {
    fn default() -> Self {
        Self::new()
    }
}

impl OutputFormatter for JsonFormatter {
    fn format_table(&self, _title: &str, headers: &[&str], rows: &[Vec<String>]) -> String {
        let objects: Vec<serde_json::Value> = rows
            .iter()
            .map(|row| {
                let mut map = serde_json::Map::new();
                for (i, header) in headers.iter().enumerate() {
                    let value = row.get(i).cloned().unwrap_or_default();
                    map.insert(header.to_string(), serde_json::Value::String(value));
                }
                serde_json::Value::Object(map)
            })
            .collect();

        serde_json::to_string_pretty(&objects).unwrap_or_else(|_| "[]".to_string())
    }

    fn format_list(&self, title: &str, items: &[(&str, &str)]) -> String {
        let mut map = serde_json::Map::new();
        map.insert(
            "title".to_string(),
            serde_json::Value::String(title.to_string()),
        );

        let entries: serde_json::Map<String, serde_json::Value> = items
            .iter()
            .map(|(k, v)| (k.to_string(), serde_json::Value::String(v.to_string())))
            .collect();
        map.insert("data".to_string(), serde_json::Value::Object(entries));

        serde_json::to_string_pretty(&map).unwrap_or_else(|_| "{}".to_string())
    }

    fn format_tree(&self, items: &[TreeItem]) -> String {
        let tree_items: Vec<serde_json::Value> = items
            .iter()
            .map(|item| {
                let mut map = serde_json::Map::new();
                map.insert(
                    "label".to_string(),
                    serde_json::Value::String(item.label.clone()),
                );
                map.insert(
                    "depth".to_string(),
                    serde_json::Value::Number(serde_json::Number::from(item.depth as u64)),
                );
                serde_json::Value::Object(map)
            })
            .collect();

        serde_json::to_string_pretty(&tree_items).unwrap_or_else(|_| "[]".to_string())
    }

    fn format_stats(&self, label: &str, items: &[(String, usize)]) -> String {
        let mut map = serde_json::Map::new();
        map.insert(
            "label".to_string(),
            serde_json::Value::String(label.to_string()),
        );

        let entries: Vec<serde_json::Value> = items
            .iter()
            .map(|(name, count)| {
                let mut m = serde_json::Map::new();
                m.insert("name".to_string(), serde_json::Value::String(name.clone()));
                m.insert(
                    "count".to_string(),
                    serde_json::Value::Number(serde_json::Number::from(*count as u64)),
                );
                serde_json::Value::Object(m)
            })
            .collect();
        map.insert("items".to_string(), serde_json::Value::Array(entries));

        serde_json::to_string_pretty(&map).unwrap_or_else(|_| "{}".to_string())
    }

    fn format_error(&self, msg: &str) -> String {
        let mut map = serde_json::Map::new();
        map.insert(
            "error".to_string(),
            serde_json::Value::String(msg.to_string()),
        );
        serde_json::to_string_pretty(&map).unwrap_or_else(|_| "{}".to_string())
    }

    fn format_success(&self, msg: &str) -> String {
        let mut map = serde_json::Map::new();
        map.insert(
            "status".to_string(),
            serde_json::Value::String("success".to_string()),
        );
        map.insert(
            "message".to_string(),
            serde_json::Value::String(msg.to_string()),
        );
        serde_json::to_string_pretty(&map).unwrap_or_else(|_| "{}".to_string())
    }

    fn format_info(&self, msg: &str) -> String {
        let mut map = serde_json::Map::new();
        map.insert(
            "info".to_string(),
            serde_json::Value::String(msg.to_string()),
        );
        serde_json::to_string_pretty(&map).unwrap_or_else(|_| "{}".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_table_json() {
        let fmt = JsonFormatter::new();
        let headers = &["name", "value"];
        let rows = vec![
            vec!["foo".to_string(), "42".to_string()],
            vec!["bar".to_string(), "7".to_string()],
        ];

        let output = fmt.format_table("Test", headers, &rows);
        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert!(parsed.is_array());
        let arr = parsed.as_array().unwrap();
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0]["name"], "foo");
        assert_eq!(arr[0]["value"], "42");
    }

    #[test]
    fn test_format_list_json() {
        let fmt = JsonFormatter::new();
        let items = vec![("key1", "val1"), ("key2", "val2")];

        let output = fmt.format_list("Info", &items);
        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(parsed["title"], "Info");
        assert_eq!(parsed["data"]["key1"], "val1");
    }

    #[test]
    fn test_format_error_json() {
        let fmt = JsonFormatter::new();
        let output = fmt.format_error("something broke");
        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(parsed["error"], "something broke");
    }

    #[test]
    fn test_format_stats_json() {
        let fmt = JsonFormatter::new();
        let items = vec![("Rust".to_string(), 100), ("Go".to_string(), 50)];
        let output = fmt.format_stats("Languages", &items);
        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(parsed["label"], "Languages");
        assert_eq!(parsed["items"][0]["name"], "Rust");
        assert_eq!(parsed["items"][0]["count"], 100);
    }
}

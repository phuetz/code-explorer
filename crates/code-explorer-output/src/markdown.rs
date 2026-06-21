//! Markdown output formatter optimized for LLM consumption.
//!
//! Produces clean, parseable Markdown with tables, headers, and code blocks.
//! Useful for piping output into AI tools that consume Markdown.

use crate::traits::{OutputFormatter, TreeItem};

/// Markdown output formatter.
pub struct MarkdownFormatter;

impl MarkdownFormatter {
    pub fn new() -> Self {
        Self
    }
}

impl Default for MarkdownFormatter {
    fn default() -> Self {
        Self::new()
    }
}

impl OutputFormatter for MarkdownFormatter {
    fn format_table(&self, title: &str, headers: &[&str], rows: &[Vec<String>]) -> String {
        let mut out = String::new();

        out.push_str(&format!("## {}\n\n", title));

        if rows.is_empty() {
            out.push_str("_No data._\n\n");
            return out;
        }

        // Header row
        out.push_str("| ");
        for (i, header) in headers.iter().enumerate() {
            if i > 0 {
                out.push_str(" | ");
            }
            // Escape pipes in headers too — without this, a header like
            // `count|total` would split into two columns and corrupt the table.
            out.push_str(&header.replace('|', "\\|"));
        }
        out.push_str(" |\n");

        // Separator
        out.push_str("| ");
        for (i, _) in headers.iter().enumerate() {
            if i > 0 {
                out.push_str(" | ");
            }
            out.push_str("---");
        }
        out.push_str(" |\n");

        // Data rows
        for row in rows {
            out.push_str("| ");
            for (i, cell) in row.iter().enumerate() {
                if i > 0 {
                    out.push_str(" | ");
                }
                // Escape pipe characters in cell content
                out.push_str(&cell.replace('|', "\\|"));
            }
            out.push_str(" |\n");
        }
        out.push('\n');

        out
    }

    fn format_list(&self, title: &str, items: &[(&str, &str)]) -> String {
        let mut out = String::new();

        out.push_str(&format!("## {}\n\n", title));

        for (key, value) in items {
            out.push_str(&format!("- **{}**: {}\n", key, value));
        }
        out.push('\n');

        out
    }

    fn format_tree(&self, items: &[TreeItem]) -> String {
        let mut out = String::new();
        out.push_str("```\n");

        for item in items {
            let indent = "  ".repeat(item.depth);
            let connector = if item.depth == 0 {
                String::new()
            } else if item.is_last {
                "\u{2514}\u{2500}\u{2500} ".to_string()
            } else {
                "\u{251C}\u{2500}\u{2500} ".to_string()
            };
            out.push_str(&format!("{}{}{}\n", indent, connector, item.label));
        }

        out.push_str("```\n\n");
        out
    }

    fn format_stats(&self, label: &str, items: &[(String, usize)]) -> String {
        let mut out = String::new();

        out.push_str(&format!("## {}\n\n", label));

        if items.is_empty() {
            out.push_str("_No data._\n\n");
            return out;
        }

        out.push_str("| Name | Count |\n");
        out.push_str("| --- | --- |\n");

        for (name, count) in items {
            // Escape pipe so a name like `src/a|b` doesn't introduce spurious
            // table columns (`format_table` already does this; this method
            // had been the only outlier).
            out.push_str(&format!("| {} | {} |\n", name.replace('|', "\\|"), count));
        }
        out.push('\n');

        out
    }

    fn format_error(&self, msg: &str) -> String {
        format!("> **Error:** {}\n", msg)
    }

    fn format_success(&self, msg: &str) -> String {
        format!("> **Success:** {}\n", msg)
    }

    fn format_info(&self, msg: &str) -> String {
        format!("> **Info:** {}\n", msg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_table_markdown() {
        let fmt = MarkdownFormatter::new();
        let headers = &["File", "Score"];
        let rows = vec![
            vec!["main.rs".to_string(), "0.95".to_string()],
            vec!["lib.rs".to_string(), "0.30".to_string()],
        ];

        let output = fmt.format_table("Hotspots", headers, &rows);
        assert!(output.contains("## Hotspots"));
        assert!(output.contains("| File | Score |"));
        assert!(output.contains("| --- | --- |"));
        assert!(output.contains("| main.rs | 0.95 |"));
    }

    #[test]
    fn test_format_list_markdown() {
        let fmt = MarkdownFormatter::new();
        let items = vec![("Author", "Alice"), ("Files", "42")];

        let output = fmt.format_list("Summary", &items);
        assert!(output.contains("## Summary"));
        assert!(output.contains("- **Author**: Alice"));
    }

    #[test]
    fn test_format_tree_markdown() {
        let fmt = MarkdownFormatter::new();
        let items = vec![
            TreeItem {
                label: "root".to_string(),
                depth: 0,
                is_last: false,
            },
            TreeItem {
                label: "child".to_string(),
                depth: 1,
                is_last: true,
            },
        ];

        let output = fmt.format_tree(&items);
        assert!(output.contains("```"));
        assert!(output.contains("root"));
        assert!(output.contains("child"));
    }

    #[test]
    fn test_pipe_char_escaping() {
        let fmt = MarkdownFormatter::new();
        let headers = &["Name"];
        let rows = vec![vec!["foo|bar".to_string()]];

        let output = fmt.format_table("Test", headers, &rows);
        assert!(output.contains("foo\\|bar"));
    }
}

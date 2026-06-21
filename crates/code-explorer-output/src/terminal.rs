//! Colored terminal output with Unicode box-drawing characters.
//!
//! Respects the `NO_COLOR` environment variable (see <https://no-color.org/>).

use colored::Colorize;

use crate::traits::{OutputFormatter, TreeItem};

/// Approximate display width of a string in terminal columns.
/// Counts Unicode scalar values rather than bytes so Latin/Cyrillic/etc. accents
/// pad correctly. Does not account for CJK fullwidth or zero-width combining
/// marks, but is a meaningful improvement over `str::len()` (byte count).
fn display_width(s: &str) -> usize {
    s.chars().count()
}

/// Pad `s` on the right to `width` columns using `display_width` for sizing.
fn pad_right(s: &str, width: usize) -> String {
    let w = display_width(s);
    if w >= width {
        s.to_string()
    } else {
        let mut out = String::with_capacity(s.len() + (width - w));
        out.push_str(s);
        for _ in 0..(width - w) {
            out.push(' ');
        }
        out
    }
}

/// Terminal formatter that produces colored output with Unicode box-drawing.
pub struct TerminalFormatter {
    no_color: bool,
}

impl TerminalFormatter {
    pub fn new() -> Self {
        let no_color = std::env::var("NO_COLOR").is_ok();
        if no_color {
            colored::control::set_override(false);
        }
        Self { no_color }
    }

    /// Check if colors are disabled.
    #[allow(dead_code)]
    pub fn is_no_color(&self) -> bool {
        self.no_color
    }
}

impl Default for TerminalFormatter {
    fn default() -> Self {
        Self::new()
    }
}

impl OutputFormatter for TerminalFormatter {
    fn format_table(&self, title: &str, headers: &[&str], rows: &[Vec<String>]) -> String {
        if rows.is_empty() {
            return format!("  {}\n  (no data)\n", title.bold().cyan());
        }

        // Calculate column widths using display width (chars), not byte length
        let col_count = headers.len();
        let mut widths: Vec<usize> = headers.iter().map(|h| display_width(h)).collect();

        for row in rows {
            for (i, cell) in row.iter().enumerate() {
                if i < col_count {
                    widths[i] = widths[i].max(display_width(cell));
                }
            }
        }

        let mut out = String::new();

        // Title
        out.push_str(&format!("\n  {}\n", title.bold().cyan()));

        // Top border: ┌─────┬─────┐
        out.push_str("  \u{250C}");
        for (i, w) in widths.iter().enumerate() {
            out.push_str(&"\u{2500}".repeat(w + 2));
            if i < col_count - 1 {
                out.push('\u{252C}');
            }
        }
        out.push_str("\u{2510}\n");

        // Header row: │ hdr │ hdr │
        out.push_str("  \u{2502}");
        for (i, header) in headers.iter().enumerate() {
            // Pad first using display width, then colorize so ANSI escapes
            // don't break alignment. The format!{:<width$} pads by bytes which
            // is wrong for multi-byte cells.
            let padded = pad_right(header, widths[i]);
            out.push_str(&format!(" {} ", padded.bold()));
            out.push('\u{2502}');
        }
        out.push('\n');

        // Header separator: ├─────┼─────┤
        out.push_str("  \u{251C}");
        for (i, w) in widths.iter().enumerate() {
            out.push_str(&"\u{2500}".repeat(w + 2));
            if i < col_count - 1 {
                out.push('\u{253C}');
            }
        }
        out.push_str("\u{2524}\n");

        // Data rows. Emit blank cells for any columns missing from the row
        // so ragged input (rows shorter than the header) still produces a
        // visually aligned box with the right border in the expected place.
        for row in rows {
            out.push_str("  \u{2502}");
            for (i, width) in widths.iter().enumerate().take(col_count) {
                let cell = row.get(i).map(|s| s.as_str()).unwrap_or("");
                let padded = pad_right(cell, *width);
                out.push_str(&format!(" {} ", padded));
                out.push('\u{2502}');
            }
            out.push('\n');
        }

        // Bottom border: └─────┴─────┘
        out.push_str("  \u{2514}");
        for (i, w) in widths.iter().enumerate() {
            out.push_str(&"\u{2500}".repeat(w + 2));
            if i < col_count - 1 {
                out.push('\u{2534}');
            }
        }
        out.push_str("\u{2518}\n");

        out
    }

    fn format_list(&self, title: &str, items: &[(&str, &str)]) -> String {
        let mut out = String::new();

        out.push_str(&format!("\n  {}\n", title.bold().cyan()));
        out.push_str(&format!("  {}\n", "\u{2500}".repeat(40).dimmed()));

        let max_key_len = items
            .iter()
            .map(|(k, _)| display_width(k))
            .max()
            .unwrap_or(0);

        for (key, value) in items {
            let padded = pad_right(key, max_key_len);
            out.push_str(&format!("  {}  {}\n", padded.yellow(), value));
        }
        out.push('\n');

        out
    }

    fn format_tree(&self, items: &[TreeItem]) -> String {
        let mut out = String::new();
        out.push('\n');

        for item in items {
            // Build the prefix based on depth
            let mut prefix = String::new();
            for _ in 0..item.depth {
                prefix.push_str("  ");
            }

            let connector = if item.depth == 0 {
                String::new()
            } else if item.is_last {
                "\u{2514}\u{2500}\u{2500} ".to_string()
            } else {
                "\u{251C}\u{2500}\u{2500} ".to_string()
            };

            out.push_str(&format!(
                "  {}{}{}\n",
                prefix.dimmed(),
                connector.dimmed(),
                item.label
            ));
        }
        out.push('\n');

        out
    }

    fn format_stats(&self, label: &str, items: &[(String, usize)]) -> String {
        let mut out = String::new();

        out.push_str(&format!("\n  {}\n", label.bold().cyan()));
        out.push_str(&format!("  {}\n", "\u{2500}".repeat(40).dimmed()));

        if items.is_empty() {
            out.push_str("  (no data)\n\n");
            return out;
        }

        let max_count = items.iter().map(|(_, c)| *c).max().unwrap_or(1);
        let max_label_len = items
            .iter()
            .map(|(l, _)| display_width(l))
            .max()
            .unwrap_or(0);
        let bar_max_width = 25;

        for (item_label, count) in items {
            let bar_len = if max_count > 0 {
                (*count as f64 / max_count as f64 * bar_max_width as f64) as usize
            } else {
                0
            };
            let bar_full = "\u{2588}".repeat(bar_len);
            let bar_empty = "\u{2591}".repeat(bar_max_width - bar_len);

            let padded_label = pad_right(item_label, max_label_len);
            out.push_str(&format!(
                "  {}  {:>6}  {}{}\n",
                padded_label.yellow(),
                count,
                bar_full.green(),
                bar_empty.dimmed(),
            ));
        }
        out.push('\n');

        out
    }

    fn format_error(&self, msg: &str) -> String {
        format!("{} {}\n", "Error:".red().bold(), msg)
    }

    fn format_success(&self, msg: &str) -> String {
        format!("{} {}\n", "\u{2714}".green(), msg)
    }

    fn format_info(&self, msg: &str) -> String {
        format!("{} {}\n", "\u{2139}".cyan(), msg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_table() {
        // Force no color for deterministic test output
        std::env::set_var("NO_COLOR", "1");
        let fmt = TerminalFormatter::new();

        let headers = &["File", "Commits", "Score"];
        let rows = vec![
            vec!["main.rs".to_string(), "42".to_string(), "0.95".to_string()],
            vec!["lib.rs".to_string(), "10".to_string(), "0.30".to_string()],
        ];

        let output = fmt.format_table("Hotspots", headers, &rows);
        assert!(output.contains("Hotspots"));
        assert!(output.contains("main.rs"));
        assert!(output.contains("42"));
        // Check box-drawing characters are present
        assert!(output.contains("\u{250C}")); // top-left
        assert!(output.contains("\u{2518}")); // bottom-right
    }

    #[test]
    fn test_format_empty_table() {
        std::env::set_var("NO_COLOR", "1");
        let fmt = TerminalFormatter::new();
        let output = fmt.format_table("Empty", &["A", "B"], &[]);
        assert!(output.contains("no data"));
    }

    #[test]
    fn test_format_stats() {
        std::env::set_var("NO_COLOR", "1");
        let fmt = TerminalFormatter::new();

        let items = vec![
            ("Rust".to_string(), 150),
            ("Python".to_string(), 80),
            ("Go".to_string(), 30),
        ];

        let output = fmt.format_stats("Language Stats", &items);
        assert!(output.contains("Language Stats"));
        assert!(output.contains("Rust"));
        assert!(output.contains("150"));
    }

    #[test]
    fn test_format_tree() {
        std::env::set_var("NO_COLOR", "1");
        let fmt = TerminalFormatter::new();

        let items = vec![
            TreeItem {
                label: "root".to_string(),
                depth: 0,
                is_last: false,
            },
            TreeItem {
                label: "child1".to_string(),
                depth: 1,
                is_last: false,
            },
            TreeItem {
                label: "child2".to_string(),
                depth: 1,
                is_last: true,
            },
        ];

        let output = fmt.format_tree(&items);
        assert!(output.contains("root"));
        assert!(output.contains("child1"));
        assert!(output.contains("child2"));
    }

    #[test]
    fn test_format_messages() {
        std::env::set_var("NO_COLOR", "1");
        let fmt = TerminalFormatter::new();

        assert!(fmt.format_error("bad").contains("bad"));
        assert!(fmt.format_success("ok").contains("ok"));
        assert!(fmt.format_info("note").contains("note"));
    }
}

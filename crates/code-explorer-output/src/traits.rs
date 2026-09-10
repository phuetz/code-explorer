/// A single item in a tree display.
#[derive(Debug, Clone)]
pub struct TreeItem {
    pub label: String,
    pub depth: usize,
    pub is_last: bool,
}

/// Trait for formatting structured output in different formats.
///
/// Implementors produce strings in their target format (terminal with colors,
/// JSON, or Markdown). The caller can choose the formatter based on flags
/// like `--json` or `--markdown`.
pub trait OutputFormatter {
    /// Format a table with a title, column headers, and rows.
    fn format_table(&self, title: &str, headers: &[&str], rows: &[Vec<String>]) -> String;

    /// Format a key-value list with a title.
    fn format_list(&self, title: &str, items: &[(&str, &str)]) -> String;

    /// Format a tree structure.
    fn format_tree(&self, items: &[TreeItem]) -> String;

    /// Format statistics with labels and counts (for bar charts / summaries).
    fn format_stats(&self, label: &str, items: &[(String, usize)]) -> String;

    /// Format an error message.
    fn format_error(&self, msg: &str) -> String;

    /// Format a success message.
    fn format_success(&self, msg: &str) -> String;

    /// Format an informational message.
    fn format_info(&self, msg: &str) -> String;
}

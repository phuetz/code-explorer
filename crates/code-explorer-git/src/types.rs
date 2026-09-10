use serde::{Deserialize, Serialize};

/// A file that has been frequently modified (a "hotspot").
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileHotspot {
    pub path: String,
    pub commit_count: u32,
    pub lines_added: u32,
    pub lines_removed: u32,
    /// Churn = lines_added + lines_removed
    pub churn: u32,
    /// Normalized hotspot score (0.0..1.0)
    pub score: f64,
    /// ISO date of last modification
    pub last_modified: String,
    pub author_count: u32,
}

/// A pair of files that frequently change together.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangeCoupling {
    pub file_a: String,
    pub file_b: String,
    pub shared_commits: u32,
    /// coupling_strength = shared_commits / max(commits_a, commits_b)
    pub coupling_strength: f64,
}

/// Ownership information for a single file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileOwnership {
    pub path: String,
    pub primary_author: String,
    pub ownership_pct: f64,
    pub author_count: u32,
    pub authors: Vec<AuthorContribution>,
}

/// An individual author's contribution to a file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthorContribution {
    pub name: String,
    pub email: String,
    pub commits: u32,
    pub pct: f64,
}

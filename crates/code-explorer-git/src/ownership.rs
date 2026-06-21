//! Analyze code ownership via git log.
//!
//! Uses commit counts per author per file (faster than git blame) to determine
//! the primary author and ownership distribution for each file.

use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

use crate::types::{AuthorContribution, FileOwnership};

/// Errors that can occur during ownership analysis.
#[derive(Debug, thiserror::Error)]
pub enum OwnershipError {
    #[error("git command failed: {0}")]
    GitCommand(String),
    #[error("not a git repository: {0}")]
    NotGitRepo(String),
}

/// Analyze code ownership for files in the repository.
///
/// Returns ownership information for each file, sorted by number of authors
/// (descending) to highlight files with distributed ownership first.
pub fn analyze_ownership(repo_path: &Path) -> Result<Vec<FileOwnership>, OwnershipError> {
    // Get commit info with author name, email, and associated files
    let log_output = Command::new("git")
        .args(["log", "--pretty=format:COMMIT:%H %an <%ae>", "--name-only"])
        .current_dir(repo_path)
        .output()
        .map_err(|e| OwnershipError::GitCommand(e.to_string()))?;

    if !log_output.status.success() {
        let stderr = String::from_utf8_lossy(&log_output.stderr);
        return Err(OwnershipError::NotGitRepo(stderr.to_string()));
    }

    let log_text = String::from_utf8_lossy(&log_output.stdout);

    // Track commits per author per file
    // file -> (author_name, author_email) -> commit_count
    let mut file_author_commits: HashMap<String, HashMap<(String, String), u32>> = HashMap::new();

    let mut current_author: Option<(String, String)> = None; // (name, email)

    for line in log_text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix("COMMIT:") {
            // Parse: HASH author_name <email>
            // Find the email in angle brackets. Require email_start < email_end,
            // otherwise a malformed line like "Foo> <Bar" would cause a slice
            // panic on `rest[email_start + 1..email_end]`.
            match (rest.rfind('<'), rest.rfind('>')) {
                (Some(email_start), Some(email_end)) if email_start < email_end => {
                    let email = rest[email_start + 1..email_end].trim().to_string();
                    // Author name is between the hash and the email
                    let after_hash = rest.split_once(' ').map(|x| x.1).unwrap_or("");
                    let name = after_hash[..after_hash.rfind('<').unwrap_or(after_hash.len())]
                        .trim()
                        .to_string();
                    current_author = Some((name, email));
                }
                _ => current_author = None,
            }
        } else if let Some(ref author) = current_author {
            // This is a filename
            let file = line.to_string();
            *file_author_commits
                .entry(file)
                .or_default()
                .entry(author.clone())
                .or_insert(0) += 1;
        }
    }

    // Build ownership entries
    let mut ownerships: Vec<FileOwnership> = Vec::new();

    for (file, author_map) in &file_author_commits {
        let total_commits: u32 = author_map.values().sum();
        if total_commits == 0 {
            continue;
        }

        // Build author contributions sorted by commit count
        let mut authors: Vec<AuthorContribution> = author_map
            .iter()
            .map(|((name, email), &count)| AuthorContribution {
                name: name.clone(),
                email: email.clone(),
                commits: count,
                pct: count as f64 / total_commits as f64 * 100.0,
            })
            .collect();

        authors.sort_by(|a, b| {
            b.pct
                .partial_cmp(&a.pct)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let primary_author = authors.first().map(|a| a.name.clone()).unwrap_or_default();
        let ownership_pct = authors.first().map(|a| a.pct).unwrap_or(0.0);

        ownerships.push(FileOwnership {
            path: file.clone(),
            primary_author,
            ownership_pct,
            author_count: authors.len() as u32,
            authors,
        });
    }

    // Sort by author_count descending (files with most distributed ownership first)
    ownerships.sort_by(|a, b| b.author_count.cmp(&a.author_count));

    Ok(ownerships)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_analyze_ownership_on_self() {
        let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap();
        if !repo.join(".git").exists() {
            return;
        }

        let result = analyze_ownership(repo);
        assert!(
            result.is_ok(),
            "analyze_ownership should not error: {:?}",
            result.err()
        );

        let ownerships = result.unwrap();
        for o in &ownerships {
            assert!(!o.path.is_empty());
            assert!(!o.primary_author.is_empty());
            assert!(o.ownership_pct > 0.0 && o.ownership_pct <= 100.0);
            assert!(o.author_count >= 1);
            assert!(!o.authors.is_empty());

            // Percentages should sum to ~100
            let total_pct: f64 = o.authors.iter().map(|a| a.pct).sum();
            assert!(
                (total_pct - 100.0).abs() < 0.1,
                "percentages should sum to 100, got {}",
                total_pct
            );
        }
    }
}

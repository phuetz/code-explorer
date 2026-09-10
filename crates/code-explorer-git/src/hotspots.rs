//! Analyze git history for file-level hotspots.
//!
//! A hotspot is a file that is frequently modified and has high code churn
//! (lines added + removed). These files are often the source of bugs and
//! maintenance burden.

use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

use crate::types::FileHotspot;

/// Errors that can occur during hotspot analysis.
#[derive(Debug, thiserror::Error)]
pub enum HotspotError {
    #[error("git command failed: {0}")]
    GitCommand(String),
    #[error("not a git repository: {0}")]
    NotGitRepo(String),
    #[error("failed to parse git output: {0}")]
    ParseError(String),
}

/// Analyze the git log for file-level hotspots within the given time window.
///
/// Files are scored by `normalize(commit_count * churn)` and returned sorted
/// by score descending.
pub fn analyze_hotspots(
    repo_path: &Path,
    since_days: u32,
) -> Result<Vec<FileHotspot>, HotspotError> {
    // Step 1: Get commit hashes with associated files
    let since_arg = format!("{} days ago", since_days);
    let log_output = Command::new("git")
        .args([
            "log",
            &format!("--since={}", since_arg),
            "--pretty=format:COMMIT:%H %aI",
            "--name-only",
        ])
        .current_dir(repo_path)
        .output()
        .map_err(|e| HotspotError::GitCommand(e.to_string()))?;

    if !log_output.status.success() {
        let stderr = String::from_utf8_lossy(&log_output.stderr);
        return Err(HotspotError::NotGitRepo(stderr.to_string()));
    }

    let log_text = String::from_utf8_lossy(&log_output.stdout);

    // Parse log: track commits per file, last modified date, and authors per file
    let mut file_commits: HashMap<String, Vec<String>> = HashMap::new(); // file -> list of commit hashes
    let mut file_last_date: HashMap<String, String> = HashMap::new();
    let mut file_authors: HashMap<String, std::collections::HashSet<String>> = HashMap::new();

    let mut current_commit: Option<String> = None;
    let mut current_date: Option<String> = None;

    for line in log_text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix("COMMIT:") {
            let parts: Vec<&str> = rest.splitn(2, ' ').collect();
            current_commit = Some(parts[0].to_string());
            current_date = parts.get(1).map(|s| s.to_string());
        } else if let Some(ref commit_hash) = current_commit {
            // This is a filename
            let file = line.to_string();
            file_commits
                .entry(file.clone())
                .or_default()
                .push(commit_hash.clone());
            // Track first date seen per file (most recent since log is reverse-chronological)
            if !file_last_date.contains_key(&file) {
                if let Some(ref date) = current_date {
                    file_last_date.insert(file.clone(), date.clone());
                }
            }
        }
    }

    // Step 2: Get numstat for additions/deletions per file
    let numstat_output = Command::new("git")
        .args([
            "log",
            &format!("--since={}", since_arg),
            "--pretty=format:COMMIT:%H %an",
            "--numstat",
        ])
        .current_dir(repo_path)
        .output()
        .map_err(|e| HotspotError::GitCommand(e.to_string()))?;

    let numstat_text = String::from_utf8_lossy(&numstat_output.stdout);

    let mut file_added: HashMap<String, u32> = HashMap::new();
    let mut file_removed: HashMap<String, u32> = HashMap::new();
    let mut current_author: Option<String> = None;

    for line in numstat_text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix("COMMIT:") {
            let parts: Vec<&str> = rest.splitn(2, ' ').collect();
            current_author = parts.get(1).map(|s| s.to_string());
        } else {
            // numstat line: <added>\t<removed>\t<file>
            // Binary files report `-` `-` for added/removed; treat as 0.
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() >= 3 {
                let added: u32 = if parts[0] == "-" {
                    0
                } else {
                    parts[0].parse().unwrap_or(0)
                };
                let removed: u32 = if parts[1] == "-" {
                    0
                } else {
                    parts[1].parse().unwrap_or(0)
                };
                let file = parts[2].to_string();
                // Saturating add prevents u32 overflow on extremely active files
                // (e.g., generated/vendored sources with millions of churned lines).
                let added_entry = file_added.entry(file.clone()).or_insert(0);
                *added_entry = added_entry.saturating_add(added);
                let removed_entry = file_removed.entry(file.clone()).or_insert(0);
                *removed_entry = removed_entry.saturating_add(removed);

                if let Some(ref author) = current_author {
                    file_authors.entry(file).or_default().insert(author.clone());
                }
            }
        }
    }

    // Step 3: Build hotspot entries
    let mut hotspots: Vec<FileHotspot> = Vec::new();

    for (file, commits) in &file_commits {
        let commit_count = commits.len() as u32;
        let lines_added = file_added.get(file).copied().unwrap_or(0);
        let lines_removed = file_removed.get(file).copied().unwrap_or(0);
        let churn = lines_added.saturating_add(lines_removed);
        let last_modified = file_last_date.get(file).cloned().unwrap_or_default();
        let author_count = file_authors.get(file).map(|s| s.len() as u32).unwrap_or(0);

        hotspots.push(FileHotspot {
            path: file.clone(),
            commit_count,
            lines_added,
            lines_removed,
            churn,
            score: 0.0, // computed below
            last_modified,
            author_count,
        });
    }

    // Step 4: Normalize scores
    // Score = commit_count * churn, then normalize to 0.0..1.0
    let raw_scores: Vec<f64> = hotspots
        .iter()
        .map(|h| h.commit_count as f64 * h.churn as f64)
        .collect();

    let max_raw = raw_scores.iter().copied().fold(0.0_f64, f64::max);

    if max_raw > 0.0 {
        for (i, hotspot) in hotspots.iter_mut().enumerate() {
            hotspot.score = raw_scores[i] / max_raw;
        }
    }

    // Sort by score descending
    hotspots.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    Ok(hotspots)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_analyze_hotspots_on_self() {
        // Test on a known git repo - the code-explorer repo itself
        // This test only works when run from within the git repo
        let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap();
        if !repo.join(".git").exists() {
            // Not in a git repo, skip
            return;
        }

        let result = analyze_hotspots(repo, 365);
        assert!(
            result.is_ok(),
            "analyze_hotspots should not error: {:?}",
            result.err()
        );

        let hotspots = result.unwrap();
        // There should be at least some files with commits
        if !hotspots.is_empty() {
            // Scores should be normalized 0..1
            assert!(hotspots[0].score <= 1.0);
            assert!(hotspots[0].score >= 0.0);
            // First entry should have the highest score
            if hotspots.len() > 1 {
                assert!(hotspots[0].score >= hotspots[1].score);
            }
        }
    }
}

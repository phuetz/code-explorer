//! Analyze temporal coupling between files.
//!
//! Temporal coupling measures how often two files change in the same commit.
//! High coupling suggests hidden dependencies that may not be visible in the
//! code structure itself.

use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

use crate::types::ChangeCoupling;

/// Errors that can occur during coupling analysis.
#[derive(Debug, thiserror::Error)]
pub enum CouplingError {
    #[error("git command failed: {0}")]
    GitCommand(String),
    #[error("not a git repository: {0}")]
    NotGitRepo(String),
}

/// Analyze temporal coupling between files in the repository.
///
/// Returns pairs of files that have been changed together in at least
/// `min_shared` commits, sorted by coupling strength descending.
///
/// `since_days` limits the git history to the last N days. Defaults to all history when `None`.
pub fn analyze_coupling(
    repo_path: &Path,
    min_shared: u32,
    since_days: Option<u32>,
) -> Result<Vec<ChangeCoupling>, CouplingError> {
    // Get commit hashes with associated files
    let mut cmd = Command::new("git");
    cmd.args(["log", "--pretty=format:COMMIT:%H", "--name-only"]);
    if let Some(days) = since_days {
        cmd.arg(format!("--since={} days ago", days));
    }
    let log_output = cmd
        .current_dir(repo_path)
        .output()
        .map_err(|e| CouplingError::GitCommand(e.to_string()))?;

    if !log_output.status.success() {
        let stderr = String::from_utf8_lossy(&log_output.stderr);
        return Err(CouplingError::NotGitRepo(stderr.to_string()));
    }

    let log_text = String::from_utf8_lossy(&log_output.stdout);

    // Parse: group files by commit
    let mut commits: Vec<Vec<String>> = Vec::new();
    let mut current_files: Vec<String> = Vec::new();

    for line in log_text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with("COMMIT:") {
            // Save previous commit's files
            if !current_files.is_empty() {
                commits.push(std::mem::take(&mut current_files));
            }
        } else {
            // This is a filename
            current_files.push(line.to_string());
        }
    }
    // Don't forget the last commit
    if !current_files.is_empty() {
        commits.push(current_files);
    }

    // Count file_total_commits and pair_counts using the SAME commit set
    // (skipping large commits everywhere) so the coupling-strength denominator
    // and numerator stay consistent. Previously file_total_commits counted all
    // commits while pair_counts excluded large ones, inflating the denominator
    // and underreporting coupling.
    let mut file_total_commits: HashMap<String, u32> = HashMap::new();
    let mut pair_counts: HashMap<(String, String), u32> = HashMap::new();

    for files in &commits {
        // Skip very large commits (likely merges or bulk renames)
        if files.len() > 50 {
            continue;
        }

        for file in files {
            *file_total_commits.entry(file.clone()).or_insert(0) += 1;
        }

        // Generate all unique pairs
        for i in 0..files.len() {
            for j in (i + 1)..files.len() {
                let (a, b) = if files[i] < files[j] {
                    (files[i].clone(), files[j].clone())
                } else {
                    (files[j].clone(), files[i].clone())
                };
                *pair_counts.entry((a, b)).or_insert(0) += 1;
            }
        }
    }

    // Build results, filtering by min_shared
    let mut couplings: Vec<ChangeCoupling> = pair_counts
        .into_iter()
        .filter(|(_, count)| *count >= min_shared)
        .map(|((file_a, file_b), shared_commits)| {
            let commits_a = file_total_commits.get(&file_a).copied().unwrap_or(1);
            let commits_b = file_total_commits.get(&file_b).copied().unwrap_or(1);
            let max_commits = commits_a.max(commits_b);
            let coupling_strength = if max_commits > 0 {
                shared_commits as f64 / max_commits as f64
            } else {
                0.0
            };

            ChangeCoupling {
                file_a,
                file_b,
                shared_commits,
                coupling_strength,
            }
        })
        .collect();

    // Sort by coupling strength descending
    couplings.sort_by(|a, b| {
        b.coupling_strength
            .partial_cmp(&a.coupling_strength)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    Ok(couplings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_analyze_coupling_on_self() {
        let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap();
        if !repo.join(".git").exists() {
            return;
        }

        let result = analyze_coupling(repo, 2, Some(180));
        assert!(
            result.is_ok(),
            "analyze_coupling should not error: {:?}",
            result.err()
        );

        let couplings = result.unwrap();
        for c in &couplings {
            assert!(c.coupling_strength >= 0.0 && c.coupling_strength <= 1.0);
            assert!(c.shared_commits >= 2);
            assert!(c.file_a < c.file_b, "pairs should be canonically ordered");
        }
        // If there are results, they should be sorted by strength
        if couplings.len() > 1 {
            assert!(couplings[0].coupling_strength >= couplings[1].coupling_strength);
        }
    }
}

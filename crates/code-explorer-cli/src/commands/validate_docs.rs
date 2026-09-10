//! `code-explorer validate-docs` CLI wrapper.
//!
//! The actual checks live in `code_explorer_rag::validator` so the desktop chat
//! tool `validate_sfd` can run them too. This module is just the CLI front:
//! parse args, call the lib, write the JSON sidecar, print to stdout, and
//! set the exit code so CI pipelines can fail-fast on RED issues.

use std::path::PathBuf;

use anyhow::{Context, Result};
use colored::Colorize;

use code_explorer_rag::validator::validate;
pub use code_explorer_rag::validator::{Severity, ValidationReport};

pub fn run(repo_path: Option<&str>, docs_dir_override: Option<&str>, json: bool) -> Result<()> {
    let repo = match repo_path {
        Some(p) => PathBuf::from(p),
        None => std::env::current_dir().context("cwd")?,
    };
    // `--docs-dir` lets us validate the LIVRABLES output dir or any other
    // generated docs tree, not just the canonical `.codeexplorer/docs/` location.
    // Useful right after `code-explorer generate docx --output-dir <livrables>` to
    // gate delivery on the actual artefact, not the source dir.
    let docs_dir = match docs_dir_override {
        Some(d) => PathBuf::from(d),
        None => repo.join(".codeexplorer").join("docs"),
    };

    let report = validate(&docs_dir, &repo.display().to_string())?;

    // Persist a JSON sidecar for CI / scripted consumption.
    let meta_dir = docs_dir.join("_meta");
    std::fs::create_dir_all(&meta_dir).ok();
    let json_path = meta_dir.join("validation.json");
    let json_str = serde_json::to_string_pretty(&report)?;
    std::fs::write(&json_path, &json_str)?;

    if json {
        println!("{}", json_str);
        return Ok(());
    }

    print_console_report(&report, &json_path);

    // Exit non-zero if any RED issue: lets CI / shell pipelines fail-fast.
    if report.red_count > 0 {
        std::process::exit(2);
    }
    Ok(())
}

fn print_console_report(report: &ValidationReport, json_path: &std::path::Path) {
    println!();
    println!(
        "{} Validation report for {}",
        "==".bold().blue(),
        report.repo.bold()
    );
    println!(
        "{} pages scanned, {} with issues  ({} red, {} yellow)",
        report.total_pages,
        report.pages_with_issues,
        report.red_count.to_string().red().bold(),
        report.yellow_count.to_string().yellow().bold(),
    );

    if report.pages.is_empty() {
        println!("{} No issues found — ready to ship.", "OK".green().bold());
        println!("Report written to: {}", json_path.display());
        return;
    }

    let mut sorted = report.pages.clone();
    sorted.sort_by(|a, b| b.issues.len().cmp(&a.issues.len()));
    println!();
    for page in sorted.iter().take(10) {
        println!("  {} — {} issue(s)", page.path.bold(), page.issues.len());
        for iss in page.issues.iter().take(5) {
            let sev = match iss.severity {
                Severity::Red => "RED   ".red(),
                Severity::Yellow => "YELLOW".yellow(),
            };
            let line = iss
                .line
                .map(|n| format!("L{}", n))
                .unwrap_or_else(|| "-".to_string());
            println!("    [{}] {:>4} {}: {}", sev, line, iss.kind, iss.detail);
        }
        if page.issues.len() > 5 {
            println!("    ... +{} more", page.issues.len() - 5);
        }
    }
    if sorted.len() > 10 {
        println!();
        println!("  ... +{} more pages with issues", sorted.len() - 10);
    }

    println!();
    println!("Issue counts by kind:");
    for (kind, count) in &report.by_kind {
        println!("  {:>3}  {}", count, kind);
    }
    println!();
    println!("Full report: {}", json_path.display());

    if report.red_count > 0 {
        println!(
            "{} {} red-severity issues block delivery — fix or accept and re-run.",
            "BLOCK".red().bold(),
            report.red_count
        );
    }
}

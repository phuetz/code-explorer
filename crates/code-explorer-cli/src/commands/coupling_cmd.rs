//! CLI command: `code-explorer coupling`

use std::path::PathBuf;

use colored::Colorize;
use code_explorer_git::coupling::analyze_coupling;
use code_explorer_output::terminal::TerminalFormatter;
use code_explorer_output::traits::OutputFormatter;

pub fn run(min_shared: u32, path: Option<&str>, json: bool) -> anyhow::Result<()> {
    let repo_path = match path {
        Some(p) => PathBuf::from(p)
            .canonicalize()
            .unwrap_or_else(|_| PathBuf::from(p)),
        None => std::env::current_dir()?,
    };

    let couplings = analyze_coupling(&repo_path, min_shared, Some(180))
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    if couplings.is_empty() {
        if json {
            println!("[]");
        } else {
            eprintln!(
                "{} No file pairs found with at least {} shared commits.",
                "Info:".cyan().bold(),
                min_shared
            );
        }
        return Ok(());
    }

    if json {
        let json_output = serde_json::to_string_pretty(&couplings)?;
        println!("{}", json_output);
        return Ok(());
    }

    let fmt = TerminalFormatter::new();

    let headers = &["File A", "File B", "Shared Commits", "Strength"];
    let rows: Vec<Vec<String>> = couplings
        .iter()
        .take(30)
        .map(|c| {
            vec![
                c.file_a.clone(),
                c.file_b.clone(),
                c.shared_commits.to_string(),
                format!("{:.2}", c.coupling_strength),
            ]
        })
        .collect();

    let title = format!("Temporal Coupling (min {} shared commits)", min_shared);
    print!("{}", fmt.format_table(&title, headers, &rows));

    if couplings.len() > 30 {
        eprintln!(
            "  ... and {} more pairs (use --json for full output)",
            (couplings.len() - 30).to_string().dimmed()
        );
    }

    Ok(())
}

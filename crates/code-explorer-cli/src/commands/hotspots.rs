//! CLI command: `code-explorer hotspots`

use std::path::PathBuf;

use colored::Colorize;
use code_explorer_git::hotspots::analyze_hotspots;
use code_explorer_output::terminal::TerminalFormatter;
use code_explorer_output::traits::OutputFormatter;

pub fn run(since_days: u32, path: Option<&str>, json: bool) -> anyhow::Result<()> {
    let repo_path = match path {
        Some(p) => PathBuf::from(p)
            .canonicalize()
            .unwrap_or_else(|_| PathBuf::from(p)),
        None => std::env::current_dir()?,
    };

    let hotspots =
        analyze_hotspots(&repo_path, since_days).map_err(|e| anyhow::anyhow!("{}", e))?;

    if hotspots.is_empty() {
        if json {
            println!("[]");
        } else {
            eprintln!(
                "{} No file changes found in the last {} days.",
                "Info:".cyan().bold(),
                since_days
            );
        }
        return Ok(());
    }

    if json {
        let json_output = serde_json::to_string_pretty(&hotspots)?;
        println!("{}", json_output);
        return Ok(());
    }

    let fmt = TerminalFormatter::new();

    let headers = &["File", "Commits", "Churn", "Authors", "Score"];
    let rows: Vec<Vec<String>> = hotspots
        .iter()
        .take(30)
        .map(|h| {
            vec![
                h.path.clone(),
                h.commit_count.to_string(),
                h.churn.to_string(),
                h.author_count.to_string(),
                format!("{:.2}", h.score),
            ]
        })
        .collect();

    let title = format!("Hotspots (last {} days)", since_days);
    print!("{}", fmt.format_table(&title, headers, &rows));

    if hotspots.len() > 30 {
        eprintln!(
            "  ... and {} more files (use --json for full output)",
            (hotspots.len() - 30).to_string().dimmed()
        );
    }

    Ok(())
}

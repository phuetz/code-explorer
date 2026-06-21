//! CLI command: `code-explorer ownership`

use std::path::PathBuf;

use colored::Colorize;
use code_explorer_git::ownership::analyze_ownership;
use code_explorer_output::terminal::TerminalFormatter;
use code_explorer_output::traits::OutputFormatter;

pub fn run(path: Option<&str>, json: bool) -> anyhow::Result<()> {
    let repo_path = match path {
        Some(p) => PathBuf::from(p)
            .canonicalize()
            .unwrap_or_else(|_| PathBuf::from(p)),
        None => std::env::current_dir()?,
    };

    let ownerships = analyze_ownership(&repo_path).map_err(|e| anyhow::anyhow!("{}", e))?;

    if ownerships.is_empty() {
        if json {
            println!("[]");
        } else {
            eprintln!("{} No ownership data found.", "Info:".cyan().bold(),);
        }
        return Ok(());
    }

    if json {
        let json_output = serde_json::to_string_pretty(&ownerships)?;
        println!("{}", json_output);
        return Ok(());
    }

    let fmt = TerminalFormatter::new();

    let headers = &["File", "Primary Author", "Ownership %", "Authors"];
    let rows: Vec<Vec<String>> = ownerships
        .iter()
        .take(30)
        .map(|o| {
            vec![
                o.path.clone(),
                o.primary_author.clone(),
                format!("{:.0}%", o.ownership_pct),
                o.author_count.to_string(),
            ]
        })
        .collect();

    print!("{}", fmt.format_table("File Ownership", headers, &rows));

    if ownerships.len() > 30 {
        eprintln!(
            "  ... and {} more files (use --json for full output)",
            (ownerships.len() - 30).to_string().dimmed()
        );
    }

    Ok(())
}

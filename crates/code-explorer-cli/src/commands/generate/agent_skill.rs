//! Codex/Claude skill generator for Code Explorer-powered repositories.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::Path;

use anyhow::Result;
use colored::Colorize;
use tracing::info;

use code_explorer_core::graph::KnowledgeGraph;

use super::utils::{collect_communities, collect_language_stats, count_files};

pub(super) fn generate_agent_skill(graph: &KnowledgeGraph, repo_path: &Path) -> Result<()> {
    let repo_name = repo_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("repository");
    let markdown = render_code_explorer_agent_skill(repo_name, repo_path, graph);

    let targets = [
        repo_path
            .join(".codex")
            .join("skills")
            .join("code-explorer")
            .join("SKILL.md"),
        repo_path
            .join(".agents")
            .join("skills")
            .join("code-explorer")
            .join("SKILL.md"),
    ];

    for target in targets {
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut file = std::fs::File::create(&target)?;
        file.write_all(markdown.as_bytes())?;
        println!("  {} {}", "OK".green(), target.display());
    }

    info!("Generated Code Explorer agent skill for {}", repo_path.display());
    Ok(())
}

fn render_code_explorer_agent_skill(
    repo_name: &str,
    repo_path: &Path,
    graph: &KnowledgeGraph,
) -> String {
    let file_count = count_files(graph);
    let languages = collect_language_stats(graph);
    let communities = collect_communities(graph);
    let binary_hint = "Use `code-explorer` when it is on PATH; otherwise use `cargo run -p code-explorer-cli --` from the Code Explorer repository.";
    let mut md = String::new();

    md.push_str("---\n");
    md.push_str("name: code-explorer\n");
    md.push_str("description: Query and document this repository with the Code Explorer knowledge graph. Use for symbol lookup, impact analysis, diagrams, documentation generation, GraphRAG, and verified code answers.\n");
    md.push_str("argument-hint: \"[question|command]\"\n");
    md.push_str("allowed-tools: Bash(code-explorer *), Bash(*/code-explorer *), Bash(*/code-explorer.exe *), Bash(cargo run -p code-explorer-cli -- *), Read, Grep, Glob\n");
    md.push_str("---\n\n");

    md.push_str("# Code Explorer Repository Skill\n\n");
    md.push_str("This repository has a Code Explorer knowledge graph. Prefer Code Explorer for code intelligence before doing broad manual searches.\n\n");
    md.push_str("## Repository\n\n");
    md.push_str(&format!("- Name: `{repo_name}`\n"));
    md.push_str(&format!("- Path: `{}`\n", display_repo_path(repo_path)));
    md.push_str(&format!("- Indexed source files: `{file_count}`\n"));
    if !languages.is_empty() {
        let language_summary = languages
            .iter()
            .take(8)
            .map(|(language, count)| format!("{language} ({count})"))
            .collect::<Vec<_>>()
            .join(", ");
        md.push_str(&format!("- Languages: {language_summary}\n"));
    }
    if !communities.is_empty() {
        md.push_str(&format!("- Detected modules: `{}`\n", communities.len()));
    }
    md.push('\n');

    md.push_str("## Binary\n\n");
    md.push_str(binary_hint);
    md.push_str("\n\n");

    md.push_str("## Default Workflow\n\n");
    md.push_str("1. Check the index: `code-explorer status`.\n");
    md.push_str("2. If missing or stale, run: `code-explorer analyze <repo-path> --incremental`.\n");
    md.push_str("3. For a direct question, start with `code-explorer query`, then `code-explorer context` or `code-explorer impact` on the best symbol.\n");
    md.push_str("4. For algorithm or business-flow answers, verify with `code-explorer trace-files`, `code-explorer diagram`, and targeted source reads.\n");
    md.push_str("5. Cite only paths returned by Code Explorer or files you actually read.\n\n");

    md.push_str("## Code Intelligence Commands\n\n");
    md.push_str("- Search: `code-explorer query \"payment formula\" --limit 10`\n");
    md.push_str("- Symbol context: `code-explorer context CourrierController`\n");
    md.push_str("- Impact: `code-explorer impact CourrierController --direction both`\n");
    md.push_str("- Related files: `code-explorer trace-files CourrierController --depth 3 --json`\n");
    md.push_str("- Diagram: `code-explorer diagram CourrierController --type flowchart`\n");
    md.push_str("- Coverage/dead code: `code-explorer coverage --json`\n");
    md.push_str("- Health report: `code-explorer report --json`\n\n");

    md.push_str("## Documentation Workflows\n\n");
    md.push_str("- Markdown docs: `code-explorer generate docs --path <repo-path>`\n");
    md.push_str("- HTML wiki: `code-explorer generate html --path <repo-path>`\n");
    md.push_str("- Enriched HTML: `code-explorer generate html --path <repo-path> --enrich --enrich-profile strict --enrich-lang fr`\n");
    md.push_str("- DOCX: `code-explorer generate docx --path <repo-path>`\n");
    md.push_str("- Native PDF: `code-explorer generate pdf --path <repo-path>`\n");
    md.push_str("- Obsidian vault: `code-explorer generate obsidian --path <repo-path>`\n");
    md.push_str("- Everything: `code-explorer generate all --path <repo-path> --enrich --enrich-profile strict --enrich-lang fr`\n");
    md.push_str("- Validate deliverables: `code-explorer validate-docs --repo <repo-path> --json`\n\n");

    md.push_str("## GraphRAG And Specifications\n\n");
    md.push_str(
        "- Import Markdown/DOCX specs: `code-explorer rag-import <docs-dir> --path <repo-path>`.\n",
    );
    md.push_str(
        "- Then ask cross-code/spec questions with `code-explorer ask \"...\" --path <repo-path>`.\n",
    );
    md.push_str("- Treat generated docs as helpful evidence, but verify precise code claims with source files.\n\n");

    if !communities.is_empty() {
        md.push_str("## Important Modules\n\n");
        for (label, member_count, description) in important_modules(&communities) {
            md.push_str(&format!("- **{label}**: {member_count} symbols"));
            if let Some(desc) = description {
                md.push_str(&format!(" - {desc}"));
            }
            md.push('\n');
        }
        md.push('\n');
    }

    md.push_str("## Answering Rules\n\n");
    md.push_str("- Prefer graph evidence over guesses.\n");
    md.push_str("- Use Mermaid diagrams for workflows, dependencies, and class relationships.\n");
    md.push_str("- For long answers, split the work: find symbols, trace files, read precise methods, then synthesize.\n");
    md.push_str("- If Code Explorer cannot find a symbol, say that explicitly and list the exact searches performed.\n");

    md
}

fn display_repo_path(repo_path: &Path) -> String {
    repo_path
        .to_string_lossy()
        .trim_start_matches(r"\\?\")
        .replace('\\', "/")
}

fn important_modules(
    communities: &BTreeMap<String, super::utils::CommunityInfo>,
) -> Vec<(String, usize, Option<String>)> {
    let mut grouped: BTreeMap<String, (usize, Option<String>)> = BTreeMap::new();
    for info in communities.values() {
        let entry = grouped.entry(info.label.clone()).or_insert((0, None));
        entry.0 += info.member_ids.len();
        if entry.1.is_none() {
            entry.1 = info
                .description
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string);
        }
    }

    let mut modules = grouped
        .into_iter()
        .map(|(label, (count, desc))| (label, count, desc))
        .collect::<Vec<_>>();
    modules.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    modules.truncate(10);
    modules
}

#[cfg(test)]
mod tests {
    use super::{display_repo_path, render_code_explorer_agent_skill};
    use code_explorer_core::graph::KnowledgeGraph;

    #[test]
    fn generated_skill_advertises_documentation_workflows() {
        let graph = KnowledgeGraph::new();
        let md = render_code_explorer_agent_skill("demo", std::path::Path::new("D:/demo"), &graph);

        assert!(md.contains("name: code-explorer"));
        assert!(md.contains("code-explorer generate html"));
        assert!(md.contains("code-explorer generate pdf"));
        assert!(md.contains("code-explorer validate-docs"));
        assert!(md.contains("code-explorer rag-import"));
    }

    #[test]
    fn display_repo_path_removes_windows_extended_prefix() {
        let displayed = display_repo_path(std::path::Path::new(r"\\?\D:\demo\repo"));

        assert_eq!(displayed, "D:/demo/repo");
    }
}

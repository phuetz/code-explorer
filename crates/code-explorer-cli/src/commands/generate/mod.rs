//! The `generate` command: produces AI context files (AGENTS.md, wiki/, skills/) from the knowledge graph.

mod agent_skill;
mod agents;
mod analytics;
mod cross_ref;
mod deployment;
mod docs;
mod enrichment;
mod functional;
mod health;
mod html;
mod inject;
mod markdown;
mod pdf;
mod process_doc;
mod prompt_audit;
mod skills;
mod utils;
mod wiki;

pub(crate) use enrichment::load_llm_config;

use std::path::Path;

use anyhow::Result;
use colored::Colorize;
use tracing::info;

use code_explorer_core::storage::repo_manager;
use code_explorer_db::snapshot;

// ─── Constants ──────────────────────────────────────────────────────────
const TARGET_CONTEXT: &str = "context";
const TARGET_AGENTS: &str = "agents";
const TARGET_AGENT_SKILL: &str = "agent-skill";
const TARGET_CODEX_SKILL: &str = "codex-skill";
const TARGET_CLAUDE_SKILL: &str = "claude-skill";
const TARGET_ASSISTANT_SKILL: &str = "assistant-skill";
const TARGET_WIKI: &str = "wiki";
const TARGET_SKILLS: &str = "skills";
const TARGET_DOCS: &str = "docs";
const TARGET_DOCX: &str = "docx";
const TARGET_HTML: &str = "html";
const TARGET_OBSIDIAN: &str = "obsidian";
const TARGET_PROCESS_DOC: &str = "process-doc";
const TARGET_PDF: &str = "pdf";
const TARGET_ALL: &str = "all";
const TARGET_INJECT: &str = "inject";

#[allow(clippy::too_many_arguments)]
pub fn run(
    what: &str,
    path: Option<&str>,
    output_dir: Option<&str>,
    enrich: bool,
    enrich_profile: &str,
    enrich_lang: &str,
    enrich_citations: bool,
    enrich_only: bool,
    retry_queue: bool,
    retry_at: Option<&str>,
    traces_dir: Option<&str>,
    input: Option<&str>,
) -> Result<()> {
    // Standalone PDF mode: no knowledge graph needed
    if what == TARGET_PDF {
        if let Some(input_path) = input {
            let input = Path::new(input_path);
            let output = if let Some(out_dir) = output_dir {
                let dir = std::path::PathBuf::from(out_dir);
                if input.is_dir() {
                    dir.join("documentation.pdf")
                } else {
                    dir.join(
                        input
                            .file_stem()
                            .and_then(|s| s.to_str())
                            .unwrap_or("output"),
                    )
                    .with_extension("pdf")
                }
            } else if input.is_dir() {
                input.join("documentation.pdf")
            } else {
                input.with_extension("pdf")
            };
            return pdf::generate_pdf_from_input(input, &output);
        }
        // Fall through to knowledge graph mode below
    }

    let repo_path = Path::new(path.unwrap_or(".")).canonicalize()?;
    let storage = repo_manager::get_storage_paths(&repo_path);
    let snap_path = snapshot::snapshot_path(&storage.storage_path);
    let graph = snapshot::load_snapshot(&snap_path)?;

    info!("Generating {} for {}", what, repo_path.display());

    let default_docs_dir = repo_path.join(".codeexplorer").join("docs");
    let docs_dir = output_dir
        .map(std::path::PathBuf::from)
        .unwrap_or(default_docs_dir);

    match what {
        TARGET_CONTEXT | TARGET_AGENTS => agents::generate_agents_md(&graph, &repo_path)?,
        TARGET_AGENT_SKILL | TARGET_CODEX_SKILL | TARGET_CLAUDE_SKILL | TARGET_ASSISTANT_SKILL => {
            agent_skill::generate_agent_skill(&graph, &repo_path)?
        }
        TARGET_WIKI => wiki::generate_wiki(&graph, &repo_path)?,
        TARGET_SKILLS => skills::generate_skills(&graph, &repo_path)?,
        TARGET_DOCS => {
            docs::generate_docs(&graph, &repo_path, &docs_dir)?;
            process_doc::generate_process_docs(
                &graph,
                &repo_path,
                &docs_dir,
                traces_dir.map(std::path::Path::new),
            )?;
            enrichment::run_enrichment_if_enabled(
                enrich,
                &graph,
                &repo_path,
                enrich_profile,
                enrich_lang,
                enrich_citations,
                &docs_dir,
                retry_at,
            )?;
            let xref_count = cross_ref::apply_cross_references(&docs_dir, &graph)?;
            if xref_count > 0 {
                println!(
                    "{} Cross-references: {} links added",
                    "OK".green(),
                    xref_count
                );
            }
            write_prompt_audit(
                &docs_dir,
                &repo_path,
                what,
                enrich,
                enrich_profile,
                enrich_lang,
                enrich_citations,
                enrich_only,
                retry_queue,
                retry_at,
                traces_dir.is_some(),
            )?;
        }
        TARGET_PROCESS_DOC => {
            process_doc::generate_process_docs(
                &graph,
                &repo_path,
                &docs_dir,
                traces_dir.map(std::path::Path::new),
            )?;
            enrichment::run_enrichment_if_enabled(
                enrich,
                &graph,
                &repo_path,
                enrich_profile,
                enrich_lang,
                enrich_citations,
                &docs_dir,
                retry_at,
            )?;
            let xref_count = cross_ref::apply_cross_references(&docs_dir, &graph)?;
            if xref_count > 0 {
                println!(
                    "{} Cross-references: {} links added",
                    "OK".green(),
                    xref_count
                );
            }
            write_prompt_audit(
                &docs_dir,
                &repo_path,
                what,
                enrich,
                enrich_profile,
                enrich_lang,
                enrich_citations,
                enrich_only,
                retry_queue,
                retry_at,
                traces_dir.is_some(),
            )?;
        }
        TARGET_DOCX => {
            docs::generate_docs(&graph, &repo_path, &docs_dir)?;
            process_doc::generate_process_docs(
                &graph,
                &repo_path,
                &docs_dir,
                traces_dir.map(std::path::Path::new),
            )?;
            enrichment::run_enrichment_if_enabled(
                enrich,
                &graph,
                &repo_path,
                enrich_profile,
                enrich_lang,
                enrich_citations,
                &docs_dir,
                retry_at,
            )?;
            let xref_count = cross_ref::apply_cross_references(&docs_dir, &graph)?;
            if xref_count > 0 {
                println!(
                    "{} Cross-references: {} links added",
                    "OK".green(),
                    xref_count
                );
            }
            write_prompt_audit(
                &docs_dir,
                &repo_path,
                what,
                enrich,
                enrich_profile,
                enrich_lang,
                enrich_citations,
                enrich_only,
                retry_queue,
                retry_at,
                traces_dir.is_some(),
            )?;
            let output_path = docs_dir.join("documentation.docx");
            let repo_name = repo_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("Project");
            super::export_docx::export_docs_as_docx(&docs_dir, &output_path, repo_name)?;
            info!("Generated DOCX documentation at {}", output_path.display());
            println!("{} Generated DOCX: {}", "OK".green(), output_path.display());
        }
        TARGET_HTML => {
            if retry_queue {
                enrichment::run_enrichment_queue_only(
                    &graph,
                    &repo_path,
                    enrich_profile,
                    enrich_lang,
                    enrich_citations,
                    &docs_dir,
                    retry_at,
                )?;
            } else {
                if !enrich_only {
                    docs::generate_docs(&graph, &repo_path, &docs_dir)?;
                    process_doc::generate_process_docs(
                        &graph,
                        &repo_path,
                        &docs_dir,
                        traces_dir.map(std::path::Path::new),
                    )?;
                }
                enrichment::run_enrichment_if_enabled(
                    enrich,
                    &graph,
                    &repo_path,
                    enrich_profile,
                    enrich_lang,
                    enrich_citations,
                    &docs_dir,
                    retry_at,
                )?;
            }
            let xref_count = cross_ref::apply_cross_references(&docs_dir, &graph)?;
            if xref_count > 0 {
                println!(
                    "{} Cross-references: {} links added",
                    "OK".green(),
                    xref_count
                );
            }
            write_prompt_audit(
                &docs_dir,
                &repo_path,
                what,
                enrich,
                enrich_profile,
                enrich_lang,
                enrich_citations,
                enrich_only,
                retry_queue,
                retry_at,
                traces_dir.is_some(),
            )?;
            html::generate_html_site(&graph, &repo_path, &docs_dir)?;
        }
        TARGET_OBSIDIAN => {
            let communities = utils::collect_communities(&graph);
            let mut output_communities = std::collections::BTreeMap::new();
            for (id, info) in communities {
                output_communities.insert(
                    id,
                    code_explorer_output::obsidian::CommunityInfo {
                        label: info.label,
                        description: info.description,
                        member_ids: info.member_ids,
                    },
                );
            }
            code_explorer_output::obsidian::generate_obsidian_vault(
                &graph,
                &docs_dir,
                &output_communities,
            )?;
            println!(
                "{} Generated Obsidian Vault in {}",
                "OK".green(),
                docs_dir.join("obsidian_vault").display()
            );
        }
        TARGET_PDF => {
            docs::generate_docs(&graph, &repo_path, &docs_dir)?;
            process_doc::generate_process_docs(
                &graph,
                &repo_path,
                &docs_dir,
                traces_dir.map(std::path::Path::new),
            )?;
            enrichment::run_enrichment_if_enabled(
                enrich,
                &graph,
                &repo_path,
                enrich_profile,
                enrich_lang,
                enrich_citations,
                &docs_dir,
                retry_at,
            )?;
            let xref_count = cross_ref::apply_cross_references(&docs_dir, &graph)?;
            if xref_count > 0 {
                println!(
                    "{} Cross-references: {} links added",
                    "OK".green(),
                    xref_count
                );
            }
            write_prompt_audit(
                &docs_dir,
                &repo_path,
                what,
                enrich,
                enrich_profile,
                enrich_lang,
                enrich_citations,
                enrich_only,
                retry_queue,
                retry_at,
                traces_dir.is_some(),
            )?;
            let output_path = docs_dir.join("documentation.pdf");
            let repo_name = repo_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("Project");
            pdf::generate_pdf_from_docs(&docs_dir, &output_path, repo_name)?;
            info!("Generated PDF documentation at {}", output_path.display());
        }
        TARGET_ALL => {
            agents::generate_agents_md(&graph, &repo_path)?;
            agent_skill::generate_agent_skill(&graph, &repo_path)?;
            wiki::generate_wiki(&graph, &repo_path)?;
            skills::generate_skills(&graph, &repo_path)?;
            docs::generate_docs(&graph, &repo_path, &docs_dir)?;
            process_doc::generate_process_docs(
                &graph,
                &repo_path,
                &docs_dir,
                traces_dir.map(std::path::Path::new),
            )?;
            enrichment::run_enrichment_if_enabled(
                enrich,
                &graph,
                &repo_path,
                enrich_profile,
                enrich_lang,
                enrich_citations,
                &docs_dir,
                retry_at,
            )?;
            let xref_count = cross_ref::apply_cross_references(&docs_dir, &graph)?;
            if xref_count > 0 {
                println!(
                    "{} Cross-references: {} links added",
                    "OK".green(),
                    xref_count
                );
            }
            write_prompt_audit(
                &docs_dir,
                &repo_path,
                what,
                enrich,
                enrich_profile,
                enrich_lang,
                enrich_citations,
                enrich_only,
                retry_queue,
                retry_at,
                traces_dir.is_some(),
            )?;
            let output_path = docs_dir.join("documentation.docx");
            let repo_name = repo_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("Project");
            super::export_docx::export_docs_as_docx(&docs_dir, &output_path, repo_name)?;
            info!("Generated DOCX documentation at {}", output_path.display());
            println!("{} Generated DOCX: {}", "OK".green(), output_path.display());
            html::generate_html_site(&graph, &repo_path, &docs_dir)?;

            let communities = utils::collect_communities(&graph);
            let mut output_communities = std::collections::BTreeMap::new();
            for (id, info) in communities {
                output_communities.insert(
                    id,
                    code_explorer_output::obsidian::CommunityInfo {
                        label: info.label,
                        description: info.description,
                        member_ids: info.member_ids,
                    },
                );
            }
            code_explorer_output::obsidian::generate_obsidian_vault(
                &graph,
                &docs_dir,
                &output_communities,
            )?;
        }
        TARGET_INJECT => {
            // Inject fragments from manifest into existing .md pages, then regenerate HTML
            let manifest_path = input
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| repo_path.join("inject.json"));
            println!("→ Injecting fragments from {} …", manifest_path.display());
            let count = inject::apply_inject(&docs_dir, &manifest_path)?;
            println!(
                "{} Injected {} fragment{}",
                "OK".green(),
                count,
                if count == 1 { "" } else { "s" }
            );
            // Regenerate HTML to reflect the new content
            let _repo_name = repo_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("Project");
            html::generate_html_site(&graph, &repo_path, &docs_dir)?;
            let html_path = docs_dir.join("index.html");
            info!("Generated HTML documentation at {}", html_path.display());
            println!(
                "{} Generated HTML documentation: {}",
                "OK".green(),
                html_path.display()
            );
        }
        _ => {
            eprintln!(
                "Unknown target: {}. Use: context, agents, agent-skill, codex-skill, claude-skill, wiki, skills, docs, docx, pdf, html, obsidian, process-doc, inject, all",
                what
            );
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn write_prompt_audit(
    docs_dir: &Path,
    repo_path: &Path,
    target: &str,
    enrich: bool,
    enrich_profile: &str,
    enrich_lang: &str,
    enrich_citations: bool,
    enrich_only: bool,
    retry_queue: bool,
    retry_at: Option<&str>,
    traces_enabled: bool,
) -> Result<()> {
    let repo_name = repo_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("repository");
    prompt_audit::write_prompt_audit(
        docs_dir,
        &prompt_audit::PromptAuditOptions {
            repo_name,
            target,
            enrich,
            enrich_profile,
            enrich_lang,
            enrich_citations,
            enrich_only,
            retry_queue,
            retry_at,
            traces_enabled,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::html::strip_html_tags;
    use super::utils::*;

    #[test]
    fn test_sanitize_filename() {
        assert_eq!(sanitize_filename("Hello World"), "hello_world");
        assert_eq!(
            sanitize_filename("DossiersController"),
            "dossierscontroller"
        );
        assert_eq!(sanitize_filename("a-b_c"), "a-b_c");
    }

    #[test]
    fn test_extract_params_from_content() {
        assert_eq!(
            extract_params_from_content("string id, int page", "test"),
            "`string` id, `int` page"
        );
        assert_eq!(extract_params_from_content("", "test"), "-");
        assert_eq!(
            extract_params_from_content("DossierPresta dossier", "test"),
            "`DossierPresta` dossier"
        );
    }

    #[test]
    fn test_strip_html_tags() {
        assert_eq!(strip_html_tags("<h1>Hello</h1><p>World</p>"), "Hello World");
        assert_eq!(strip_html_tags("no tags here"), "no tags here");
        assert_eq!(strip_html_tags("<a href='x'>link</a> text"), "link text");
        assert_eq!(strip_html_tags(""), "");
    }
}

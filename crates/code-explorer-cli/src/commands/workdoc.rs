//! CLI surface for the same "document de travail" workflow used by the React
//! chat panel: import a DOCX questionnaire, answer questions with Code Explorer
//! tools, persist a resumable JSON state, then export the final DOCX.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use colored::Colorize;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::sync::Mutex as TokioMutex;

use code_explorer_mcp::backend::local::LocalBackend;

use super::ask::{ask_question_with_tools, StreamEvent};
use super::export_docx::export_markdown_as_docx;

const PROMPT_VERSION: &str = "workdoc-cli-v1";
const LEDGER_VERSION: u32 = 1;
const CLI_MAX_HEADING_QUESTIONS: usize = 200;

#[derive(Debug, Parser)]
#[command(
    name = "code-explorer workdoc",
    about = "Process a Word questionnaire from the CLI: extract, answer, and export.",
    after_help = "Examples:\n  code-explorer workdoc extract questions.docx --output workdoc.json --extract-mode headings\n  code-explorer workdoc list questions.docx --format markdown --output questions.md\n  code-explorer workdoc status questions.docx --path D:\\taf\\sample-app\n  code-explorer workdoc run questions.docx --path D:\\taf\\sample-app --state workdoc.json --reuse-previous --output answers.docx\n  code-explorer workdoc export workdoc.json --output answers.docx"
)]
struct WorkdocCli {
    #[command(subcommand)]
    action: WorkdocCommand,
}

#[derive(Debug, Subcommand)]
pub enum WorkdocCommand {
    /// Extract questions from a DOCX and save a resumable work-document JSON.
    Extract {
        /// Source DOCX containing questions.
        docx: PathBuf,
        /// Output JSON state file. Defaults next to the DOCX.
        #[arg(short, long, alias = "state")]
        output: Option<PathBuf>,
        /// Also write the converted source Markdown next to the state file.
        #[arg(long)]
        source_markdown: Option<PathBuf>,
        /// Extraction strategy: auto prefers structured "# Question n:" headings when present.
        #[arg(long, value_enum, default_value_t = WorkdocExtractMode::Auto)]
        extract_mode: WorkdocExtractMode,
    },
    /// Extract/resume, answer questions through Code Explorer tools, and export DOCX.
    Run {
        /// Source DOCX containing questions.
        docx: PathBuf,
        /// Repository path to analyze. Defaults to current directory.
        #[arg(short, long)]
        path: Option<PathBuf>,
        /// Resumable work-document JSON state file.
        #[arg(long)]
        state: Option<PathBuf>,
        /// Final DOCX output path.
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Also write the intermediate Markdown used for DOCX export.
        #[arg(long)]
        markdown_output: Option<PathBuf>,
        /// Maximum number of non-answered questions to process in this run.
        #[arg(long)]
        limit: Option<usize>,
        /// Re-answer already answered questions.
        #[arg(long, default_value_t = false)]
        force: bool,
        /// Reuse matching answered questions found in the workdoc ledger before calling the LLM.
        #[arg(long, default_value_t = false)]
        reuse_previous: bool,
        /// Reuse answered questions from an explicit previous workdoc JSON state.
        #[arg(long = "reuse-from")]
        reuse_from: Vec<PathBuf>,
        /// Extraction strategy used when a new state must be created.
        #[arg(long, value_enum, default_value_t = WorkdocExtractMode::Auto)]
        extract_mode: WorkdocExtractMode,
    },
    /// Export an existing work-document JSON state to Markdown and/or DOCX.
    Export {
        /// Work-document JSON state file created by `workdoc extract` or `workdoc run`.
        state: PathBuf,
        /// Final DOCX output path.
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Also write the intermediate Markdown used for DOCX export.
        #[arg(long)]
        markdown_output: Option<PathBuf>,
        /// Export Markdown only, without creating DOCX.
        #[arg(long, default_value_t = false)]
        markdown_only: bool,
    },
    /// List the active extracted question set before running generation.
    List {
        /// Source DOCX or work-document JSON state file.
        input: PathBuf,
        /// Output file. Defaults to stdout.
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// List output format.
        #[arg(long, value_enum, default_value_t = WorkdocListFormat::Table)]
        format: WorkdocListFormat,
        /// Include extracted context snippets in Markdown/JSON output.
        #[arg(long, default_value_t = false)]
        context: bool,
        /// Extraction strategy used when listing a DOCX directly.
        #[arg(long, value_enum, default_value_t = WorkdocExtractMode::Auto)]
        extract_mode: WorkdocExtractMode,
    },
    /// Show hashes and previous matching generations for a DOCX or state file.
    Status {
        /// Source DOCX or work-document JSON state file.
        input: PathBuf,
        /// Repository path used to locate the repo-local workdoc ledger.
        #[arg(short, long)]
        path: Option<PathBuf>,
        /// Explicit state path when checking a DOCX.
        #[arg(long)]
        state: Option<PathBuf>,
        /// Extraction strategy used when checking a DOCX directly.
        #[arg(long, value_enum, default_value_t = WorkdocExtractMode::Auto)]
        extract_mode: WorkdocExtractMode,
    },
}

pub async fn run(command: WorkdocCommand) -> Result<()> {
    match command {
        WorkdocCommand::Extract {
            docx,
            output,
            source_markdown,
            extract_mode,
        } => {
            let state_path = output.unwrap_or_else(|| default_state_path(&docx));
            let mut document = extract_state_from_docx(&docx, None, None, extract_mode)?;
            refresh_document_hashes(&mut document, None);
            write_state(&state_path, &document)?;
            let ledger_path = update_ledger(&state_path, &document, None)?;
            if let Some(markdown_path) = source_markdown {
                write_text_file(&markdown_path, &document.source_markdown)?;
                println!(
                    "{} Source Markdown: {}",
                    "OK".green(),
                    markdown_path.display()
                );
            }
            print_extract_summary(&state_path, &document);
            println!("{} Ledger: {}", "OK".green(), ledger_path.display());
            Ok(())
        }
        WorkdocCommand::Run {
            docx,
            path,
            state,
            output,
            markdown_output,
            limit,
            force,
            reuse_previous,
            reuse_from,
            extract_mode,
        } => {
            let repo_path = path.unwrap_or(std::env::current_dir()?);
            let repo_path = repo_path
                .canonicalize()
                .unwrap_or_else(|_| repo_path.to_path_buf());
            let repo_name = repo_path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("repository")
                .to_string();
            let state_path = state.unwrap_or_else(|| default_state_path(&docx));

            let mut document = if state_path.exists() {
                println!(
                    "{} Reprise du document de travail: {}",
                    "INFO".cyan(),
                    state_path.display()
                );
                load_state(&state_path)?
            } else {
                extract_state_from_docx(&docx, Some(&repo_path), Some(&repo_name), extract_mode)?
            };
            document.repo = Some(repo_path.display().to_string());
            document.repo_name = Some(repo_name.clone());
            document.generation = WorkGenerationMetadata::from_context(Some(&repo_path));
            refresh_document_hashes(&mut document, None);
            write_state(&state_path, &document)?;

            if force {
                if reuse_previous || !reuse_from.is_empty() {
                    println!(
                        "{} Réutilisation ignorée car --force demande de régénérer les réponses.",
                        "INFO".cyan()
                    );
                }
            } else if reuse_previous || !reuse_from.is_empty() {
                let reused = reuse_previous_answers(
                    &mut document,
                    &state_path,
                    Some(&repo_path),
                    &reuse_from,
                    reuse_previous,
                )?;
                if reused > 0 {
                    refresh_document_hashes(&mut document, None);
                    write_state(&state_path, &document)?;
                    println!(
                        "{} {} réponse(s) réutilisée(s) avant génération.",
                        "OK".green(),
                        reused
                    );
                } else {
                    println!(
                        "{} Aucune réponse précédente compatible à réutiliser.",
                        "INFO".cyan()
                    );
                }
            }

            answer_document_questions(
                &mut document,
                &repo_path,
                &repo_name,
                &state_path,
                limit,
                force,
            )
            .await?;

            refresh_document_hashes(&mut document, None);
            write_state(&state_path, &document)?;
            let markdown = build_work_document_markdown(&document);
            document.outputs.markdown_sha256 = sha256_text(&markdown);
            if let Some(markdown_path) = markdown_output.as_ref() {
                write_text_file(markdown_path, &markdown)?;
                document.outputs.markdown_path = Some(path_display(markdown_path));
                println!("{} Markdown: {}", "OK".green(), markdown_path.display());
            }

            let output_path = output.unwrap_or_else(|| default_docx_output_path(&docx));
            export_markdown_as_docx_task(
                markdown,
                output_path.clone(),
                work_document_export_title(&document),
            )
            .await
            .with_context(|| format!("Impossible d'exporter le DOCX {}", output_path.display()))?;
            document.outputs.docx_path = Some(path_display(&output_path));
            document.outputs.docx_sha256 = sha256_file(&output_path)?;
            document.outputs.exported_at = Some(unix_time_ms());
            write_state(&state_path, &document)?;
            let ledger_path = update_ledger(&state_path, &document, Some(&repo_path))?;
            println!("{} DOCX: {}", "OK".green(), output_path.display());
            println!("{} State: {}", "OK".green(), state_path.display());
            println!("{} Ledger: {}", "OK".green(), ledger_path.display());
            Ok(())
        }
        WorkdocCommand::Export {
            state,
            output,
            markdown_output,
            markdown_only,
        } => {
            let mut document = load_state(&state)?;
            refresh_document_hashes(&mut document, None);
            let markdown = build_work_document_markdown(&document);
            document.outputs.markdown_sha256 = sha256_text(&markdown);
            if let Some(markdown_path) = markdown_output.as_ref() {
                write_text_file(markdown_path, &markdown)?;
                document.outputs.markdown_path = Some(path_display(markdown_path));
                println!("{} Markdown: {}", "OK".green(), markdown_path.display());
            }
            if markdown_only {
                document.outputs.exported_at = Some(unix_time_ms());
                write_state(&state, &document)?;
                let ledger_path = update_ledger(&state, &document, None)?;
                println!("{} Ledger: {}", "OK".green(), ledger_path.display());
                return Ok(());
            }
            let output_path =
                output.unwrap_or_else(|| default_docx_output_path(Path::new(&document.filename)));
            export_markdown_as_docx_task(
                markdown,
                output_path.clone(),
                work_document_export_title(&document),
            )
            .await
            .with_context(|| format!("Impossible d'exporter le DOCX {}", output_path.display()))?;
            document.outputs.docx_path = Some(path_display(&output_path));
            document.outputs.docx_sha256 = sha256_file(&output_path)?;
            document.outputs.exported_at = Some(unix_time_ms());
            write_state(&state, &document)?;
            let ledger_path = update_ledger(&state, &document, None)?;
            println!("{} DOCX: {}", "OK".green(), output_path.display());
            println!("{} Ledger: {}", "OK".green(), ledger_path.display());
            Ok(())
        }
        WorkdocCommand::List {
            input,
            output,
            format,
            context,
            extract_mode,
        } => {
            let mut document = if is_json_path(&input) {
                load_state(&input)?
            } else {
                extract_state_from_docx(&input, None, None, extract_mode)?
            };
            refresh_document_hashes(&mut document, None);
            let listing = build_question_listing(&document, format, context)?;
            if let Some(output_path) = output {
                write_text_file(&output_path, &listing)?;
                println!(
                    "{} Liste questions: {}",
                    "OK".green(),
                    output_path.display()
                );
            } else {
                println!("{listing}");
            }
            Ok(())
        }
        WorkdocCommand::Status {
            input,
            path,
            state,
            extract_mode,
        } => {
            let repo_path = path.map(|path| path.canonicalize().unwrap_or(path));
            let state_path = state.unwrap_or_else(|| {
                if is_json_path(&input) {
                    input.clone()
                } else {
                    default_state_path(&input)
                }
            });
            let mut document = if is_json_path(&input) {
                load_state(&input)?
            } else {
                extract_state_from_docx(
                    &input,
                    repo_path.as_deref(),
                    repo_path
                        .as_ref()
                        .and_then(|path| path.file_name())
                        .and_then(|name| name.to_str()),
                    extract_mode,
                )?
            };
            refresh_document_hashes(&mut document, None);
            print_status(&document, &state_path, repo_path.as_deref())?;
            Ok(())
        }
    }
}

pub async fn run_from_args(args: Vec<String>) -> Result<()> {
    let argv = std::iter::once("code-explorer workdoc".to_string()).chain(args);
    let cli = WorkdocCli::parse_from(argv);
    run(cli.action).await
}

async fn export_markdown_as_docx_task(
    markdown: String,
    output_path: PathBuf,
    title: String,
) -> Result<()> {
    tokio::task::spawn_blocking(move || export_markdown_as_docx(&markdown, &output_path, &title))
        .await
        .context("Tâche d'export DOCX interrompue")?
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum WorkdocExtractMode {
    Auto,
    Generic,
    Headings,
}

impl WorkdocExtractMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Generic => "generic",
            Self::Headings => "headings",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum WorkdocListFormat {
    Table,
    Markdown,
    Json,
}

struct WorkdocExtraction {
    mode: WorkdocExtractMode,
    questions: Vec<super::serve::WorkdocQuestion>,
}

async fn answer_document_questions(
    document: &mut WorkDocumentState,
    repo_path: &Path,
    repo_name: &str,
    state_path: &Path,
    limit: Option<usize>,
    force: bool,
) -> Result<()> {
    let mut backend = LocalBackend::new();
    backend
        .init()
        .map_err(|err| anyhow::anyhow!("Failed to initialize MCP backend: {err}"))?;
    let backend = Arc::new(TokioMutex::new(backend));

    let planned: Vec<usize> = document
        .questions
        .iter()
        .enumerate()
        .filter(|(_, question)| force || question.status != WorkQuestionStatus::Answered)
        .map(|(index, _)| index)
        .take(limit.unwrap_or(usize::MAX))
        .collect();

    if planned.is_empty() {
        println!("{} Aucune question restante à traiter.", "OK".green());
        return Ok(());
    }

    println!(
        "{} Traitement de {} question(s) dans {}",
        "INFO".cyan(),
        planned.len(),
        repo_path.display()
    );

    let mut consecutive_failures = 0usize;
    for (position, question_index) in planned.iter().enumerate() {
        let label = document.questions[*question_index].label.clone();
        let text = document.questions[*question_index].text.clone();
        println!(
            "\n{} [{}/{}] {} - {}",
            "QUESTION".blue(),
            position + 1,
            planned.len(),
            label,
            text
        );

        document.questions[*question_index].status = WorkQuestionStatus::Answering;
        document.questions[*question_index].error = None;
        write_state(state_path, document)?;

        let prompt =
            build_work_question_prompt(document, &document.questions[*question_index], repo_name);
        let stream_label = label.clone();
        let result = ask_question_with_tools(
            &prompt,
            repo_path,
            backend.clone(),
            Some(repo_name),
            Some(Box::new(move |event| match event {
                StreamEvent::ToolCallStart { name, .. } => {
                    println!(
                        "  {} {} -> {}",
                        "tool".dimmed(),
                        stream_label.dimmed(),
                        name
                    );
                }
                StreamEvent::ToolCallEnd { name, success, .. } => {
                    let status = if success { "ok".green() } else { "fail".red() };
                    println!("  {} {} {}", "tool".dimmed(), name, status);
                }
                StreamEvent::Delta(_) => {}
            })),
        )
        .await;

        match result {
            Ok((answer, _sources)) if !answer.trim().is_empty() => {
                document.questions[*question_index].status = WorkQuestionStatus::Answered;
                let normalized = normalize_answer(&answer);
                document.questions[*question_index].answer_hash = sha256_text(&normalized);
                document.questions[*question_index].answer = Some(normalized);
                document.questions[*question_index].answered_at = Some(unix_time_ms());
                document.questions[*question_index].error = None;
                consecutive_failures = 0;
                println!("{} Réponse sauvegardée pour {}", "OK".green(), label);
            }
            Ok((_answer, _sources)) => {
                document.questions[*question_index].status = WorkQuestionStatus::Error;
                document.questions[*question_index].error =
                    Some("Réponse vide retournée par le modèle.".to_string());
                consecutive_failures += 1;
                println!("{} Réponse vide pour {}", "WARN".yellow(), label);
            }
            Err(err) => {
                document.questions[*question_index].status = WorkQuestionStatus::Error;
                document.questions[*question_index].error = Some(err.to_string());
                consecutive_failures += 1;
                println!("{} {}: {}", "ERROR".red(), label, err);
            }
        }

        refresh_document_hashes(document, None);
        write_state(state_path, document)?;
        if consecutive_failures >= 3 {
            return Err(anyhow::anyhow!(
                "Traitement arrêté après 3 échecs consécutifs. Relance avec le même --state après correction."
            ));
        }
    }

    Ok(())
}

fn extract_state_from_docx(
    docx: &Path,
    repo_path: Option<&Path>,
    repo_name: Option<&str>,
    extract_mode: WorkdocExtractMode,
) -> Result<WorkDocumentState> {
    if !docx
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("docx"))
    {
        return Err(anyhow::anyhow!(
            "Seuls les documents .docx sont supportés pour workdoc."
        ));
    }

    let raw_docx = fs::read(docx)
        .with_context(|| format!("Document introuvable ou illisible: {}", docx.display()))?;
    let bytes = raw_docx.len();
    let file_sha256 = sha256_bytes(&raw_docx);
    let markdown = code_explorer_rag::docx::docx_to_markdown_with_images(docx)
        .with_context(|| format!("Impossible de convertir le DOCX {}", docx.display()))?;
    let extraction = extract_workdoc_questions_for_cli(&markdown, extract_mode);
    let questions = extraction
        .questions
        .into_iter()
        .map(|question| WorkQuestionState {
            id: question.id,
            order: question.order,
            label: question.label,
            text: question.text,
            context: question.context,
            status: WorkQuestionStatus::Pending,
            question_hash: String::new(),
            answer: None,
            answer_hash: String::new(),
            error: None,
            answered_at: None,
        })
        .collect();

    let filename = docx
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("document.docx")
        .to_string();

    Ok(WorkDocumentState {
        id: format!("cli-workdoc-{}", uuid::Uuid::new_v4()),
        filename,
        imported_at: unix_time_ms(),
        repo: repo_path.map(|path| path.display().to_string()),
        repo_name: repo_name.map(str::to_string),
        source_bytes: bytes,
        markdown_chars: markdown.chars().count(),
        source_markdown: markdown,
        questions,
        source: WorkSourceHashes {
            file_sha256,
            extraction_mode: extraction.mode.as_str().to_string(),
            ..WorkSourceHashes::default()
        },
        generation: WorkGenerationMetadata::from_context(repo_path),
        outputs: WorkOutputs::default(),
    })
}

fn extract_workdoc_questions_for_cli(
    markdown: &str,
    requested_mode: WorkdocExtractMode,
) -> WorkdocExtraction {
    match requested_mode {
        WorkdocExtractMode::Generic => WorkdocExtraction {
            mode: WorkdocExtractMode::Generic,
            questions: super::serve::extract_workdoc_questions(markdown),
        },
        WorkdocExtractMode::Headings => WorkdocExtraction {
            mode: WorkdocExtractMode::Headings,
            questions: extract_heading_workdoc_questions(markdown),
        },
        WorkdocExtractMode::Auto => {
            let heading_questions = extract_heading_workdoc_questions(markdown);
            if heading_questions.len() >= 3 {
                WorkdocExtraction {
                    mode: WorkdocExtractMode::Headings,
                    questions: heading_questions,
                }
            } else {
                WorkdocExtraction {
                    mode: WorkdocExtractMode::Generic,
                    questions: super::serve::extract_workdoc_questions(markdown),
                }
            }
        }
    }
}

fn extract_heading_workdoc_questions(markdown: &str) -> Vec<super::serve::WorkdocQuestion> {
    let lines: Vec<String> = markdown.lines().map(normalize_cli_workdoc_line).collect();
    let heading_candidates: Vec<(usize, String, String)> = lines
        .iter()
        .enumerate()
        .filter_map(|(index, line)| {
            parse_heading_question_line(line).map(|(label, text)| (index, label, text))
        })
        .collect();

    let mut label_indexes: BTreeMap<String, usize> = BTreeMap::new();
    let mut questions = Vec::new();
    for (candidate_index, (line_index, label, fallback_text)) in
        heading_candidates.iter().enumerate()
    {
        let next_line = heading_candidates
            .get(candidate_index + 1)
            .map(|(next_index, _, _)| *next_index)
            .unwrap_or(lines.len());
        let text = collect_heading_question_text(&lines, *line_index, next_line, fallback_text);
        let context = collect_heading_question_context(&lines, *line_index, next_line);
        if let Some(existing_index) = label_indexes.get(label).copied() {
            merge_repeated_heading_question(&mut questions[existing_index], &text, &context);
            continue;
        }

        let order = questions.len() + 1;
        label_indexes.insert(label.clone(), questions.len());
        questions.push(super::serve::WorkdocQuestion {
            id: format!("q-{order:03}"),
            order,
            label: label.clone(),
            text,
            context,
        });
        if questions.len() >= CLI_MAX_HEADING_QUESTIONS {
            break;
        }
    }

    questions
}

fn merge_repeated_heading_question(
    question: &mut super::serve::WorkdocQuestion,
    text: &str,
    context: &str,
) {
    if !text.trim().is_empty() && !question.text.contains(text.trim()) {
        question.text = format!("{}\n\n{}", question.text.trim(), text.trim());
    }
    if !context.trim().is_empty() && !question.context.contains(context.trim()) {
        question.context = format!("{}\n\n---\n\n{}", question.context.trim(), context.trim());
    }
}

fn parse_heading_question_line(line: &str) -> Option<(String, String)> {
    let line = line.trim();
    let heading =
        regex::Regex::new(r"^#{1,6}\s*(Question\s+\d+(?:\.\d+)?)\s*[:：—–-]\s*(.{6,})$").unwrap();
    let caps = heading.captures(line)?;
    let label = caps.get(1)?.as_str().trim().to_string();
    let text = caps
        .get(2)?
        .as_str()
        .trim()
        .trim_matches(['»', '"'])
        .trim()
        .to_string();
    Some((label, text))
}

fn collect_heading_question_text(
    lines: &[String],
    start: usize,
    end: usize,
    fallback_text: &str,
) -> String {
    let mut parts = vec![fallback_text.trim().to_string()];
    for line in lines.iter().take(end).skip(start + 1) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if is_response_heading(trimmed) || parse_heading_question_line(trimmed).is_some() {
            break;
        }
        parts.push(trimmed.to_string());
    }
    normalize_hash_text(&parts.join("\n"))
}

fn collect_heading_question_context(lines: &[String], start: usize, end: usize) -> String {
    lines
        .iter()
        .take(end)
        .skip(start)
        .filter(|line| !line.trim().is_empty() && !is_cli_markdown_image_line(line))
        .cloned()
        .collect::<Vec<_>>()
        .join("\n")
}

fn normalize_cli_workdoc_line(line: &str) -> String {
    if is_cli_markdown_image_line(line) {
        return String::new();
    }
    line.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn is_cli_markdown_image_line(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.starts_with("![") && trimmed.contains("](data:image/")
}

fn is_response_heading(line: &str) -> bool {
    let trimmed = line.trim_start_matches('#').trim();
    let lower = trimmed.to_lowercase();
    lower.starts_with("réponse")
        || lower.starts_with("reponse")
        || lower.contains("réponse détaillée")
        || lower.contains("reponse detaillee")
}

fn build_work_question_prompt(
    document: &WorkDocumentState,
    question: &WorkQuestionState,
    repo_name: &str,
) -> String {
    [
        format!("Atelier document Code Explorer : {}", document.filename),
        format!("Question extraite : {}", question.label),
        String::new(),
        format!(
            "Question à traiter : {} dans le dépôt {}",
            question.text, repo_name
        ),
        String::new(),
        "Contexte extrait du document source :".to_string(),
        if question.context.trim().is_empty() {
            "- Aucun contexte adjacent exploitable dans le document source.".to_string()
        } else {
            question.context.trim().to_string()
        },
        String::new(),
        "Consignes atelier document :".to_string(),
        "- Traite cette question comme une tâche autonome du document de travail.".to_string(),
        "- Utilise les outils Code Explorer avant de conclure et lis les fichiers nécessaires.".to_string(),
        "- Ne recopie pas une ancienne réponse du document source sans la vérifier dans le code.".to_string(),
        "- Si une recherche ne trouve rien, explique précisément les recherches effectuées et leurs limites.".to_string(),
        "- Cite les chemins exacts des fichiers réellement consultés dans une section Sources.".to_string(),
        "- Rédige une réponse exploitable dans un livrable final, pas seulement dans le chat.".to_string(),
        String::new(),
        "Structure de réponse attendue :".to_string(),
        "1. Synthèse courte : réponse directe en 3 à 6 lignes.".to_string(),
        "2. Explication détaillée : fonctionnement, responsabilité métier, cas d’utilisation.".to_string(),
        "3. Preuves dans le code : classes, méthodes, fichiers et extraits utiles.".to_string(),
        "4. Diagramme Mermaid si la question porte sur un flux, des dépendances ou un enchaînement.".to_string(),
        "5. Impacts, limites et points d’attention pour une équipe projet.".to_string(),
        "6. Sources : chemins exacts des fichiers réellement consultés.".to_string(),
        String::new(),
        "Règles de couverture des demandes :".to_string(),
        "- Si la question demande une \"section à part\", crée un titre dédié et visible, pas seulement un paragraphe intégré.".to_string(),
        "- Si la question demande \"tous\", \"toutes\", \"exhaustif\", \"écrans\", \"courriers\", \"flux\" ou \"où se trouve\", ajoute une cartographie par canal : écran/vue, contrôleur, service, calcul métier, document/courrier, flux ou batch, et indique clairement les recherches sans résultat.".to_string(),
        "- Si la question demande un nombre d’exemples, fournis au moins ce nombre d’exemples concrets et compréhensibles par un lecteur métier.".to_string(),
        "- Si la question demande les cas possibles avec des Oui/Non, produis une matrice de cas couvrant les combinaisons utiles et les effets attendus.".to_string(),
        String::new(),
        "Règles de rendu obligatoires :".to_string(),
        "- Mermaid : liens simples (`A --> B`) ou libellés explicites (`A -->|Oui| B`) ; jamais de libellé vide (`A -->|| B`).".to_string(),
        "- Booléens et valeurs techniques : écrire `true`, `false`, `null` en code inline ; ne jamais utiliser `<true>`, `<false>` ou des chevrons.".to_string(),
    ]
    .join("\n")
}

fn build_work_document_markdown(document: &WorkDocumentState) -> String {
    let quality = WorkQuality::from_document(document);
    let title = work_document_export_title(document);
    let mut lines = vec![
        format!("# {title}"),
        String::new(),
        "| Métadonnée | Valeur |".to_string(),
        "|---|---|".to_string(),
        format!(
            "| Projet | {} |",
            table_cell(document.repo_name.as_deref().or(document.repo.as_deref()).unwrap_or("non sélectionné"))
        ),
        format!("| Document source | {} |", table_cell(&document.filename)),
        format!("| Import | {} |", document.imported_at),
        format!("| Export | {} |", unix_time_ms()),
        format!("| Questions extraites | {} |", document.questions.len()),
        format!("| Questions répondues | {} |", quality.answered),
        format!("| Statut du livrable | {} |", table_cell(&quality.readiness_label())),
        String::new(),
        "> [!NOTE]".to_string(),
        format!(
            "> Statut : {}. Livrable généré depuis un document de travail Code Explorer CLI, compatible avec l’atelier Word de l’IHM.",
            quality.readiness_label()
        ),
        String::new(),
        "## Ce que contient ce livrable".to_string(),
        String::new(),
        "- Une réponse détaillée par question, organisée comme un mini-chapitre technique.".to_string(),
        "- Les sources réellement citées par les réponses et les diagrammes Mermaid détectés.".to_string(),
        "- Un contrôle qualité documentaire pour préparer la relecture finale.".to_string(),
        String::new(),
        "## Table des questions".to_string(),
        String::new(),
        "| # | Question | État | Sources | Diagrammes |".to_string(),
        "|---|---|---|---:|---:|".to_string(),
    ];

    for question in &document.questions {
        lines.push(format!(
            "| {} | {} | {} | {} | {} |",
            question.order,
            table_cell(&question.text),
            question.status.label(),
            count_source_references(question.answer.as_deref().unwrap_or("")),
            count_mermaid_diagrams(question.answer.as_deref().unwrap_or(""))
        ));
    }

    lines.extend([
        String::new(),
        "## Plan de relecture final".to_string(),
        String::new(),
    ]);
    append_review_actions(&mut lines, document);

    lines.extend([
        String::new(),
        "### Checklist avant diffusion".to_string(),
        String::new(),
        "- Toutes les questions sont répondues ou explicitement exclues.".to_string(),
        "- Chaque réponse cite les fichiers sources réellement consultés.".to_string(),
        "- Les réponses courtes ont été enrichies avec les impacts, limites et preuves utiles."
            .to_string(),
        "- Les diagrammes Mermaid importants ont été relus dans l’export HTML/PDF/DOCX."
            .to_string(),
        "- Le DOCX final a été ouvert une fois après génération.".to_string(),
        String::new(),
        "## Questions et réponses détaillées".to_string(),
        String::new(),
    ]);

    for question in &document.questions {
        lines.extend([
            format!("### Chapitre {} - {}", question.order, question.label),
            String::new(),
            "#### Question".to_string(),
            String::new(),
            format!("> {}", question.text),
            String::new(),
            "#### Trace Code Explorer".to_string(),
            String::new(),
            "| Élément | Valeur |".to_string(),
            "|---|---|".to_string(),
            format!("| État | {} |", question.status.label()),
            format!(
                "| Génération | {} |",
                question.answered_at.unwrap_or_default()
            ),
            String::new(),
        ]);

        if !question.context.trim().is_empty() {
            lines.extend([
                "#### Contexte documentaire".to_string(),
                String::new(),
                question.context.trim().to_string(),
                String::new(),
            ]);
        }

        lines.extend(["#### Réponse technique générée".to_string(), String::new()]);
        if let Some(answer) = question
            .answer
            .as_ref()
            .filter(|answer| !answer.trim().is_empty())
        {
            lines.extend([normalize_answer(answer), String::new()]);
        } else if let Some(error) = &question.error {
            lines.extend([
                "> [!WARNING]".to_string(),
                format!("> Réponse non générée : {error}"),
                String::new(),
            ]);
        } else {
            lines.extend([
                "> [!NOTE]".to_string(),
                "> Réponse non générée.".to_string(),
                String::new(),
            ]);
        }
    }

    append_source_index(&mut lines, document);
    append_quality_markdown(&mut lines, &quality);

    lines.join("\n").trim_end().to_string() + "\n"
}

fn build_question_listing(
    document: &WorkDocumentState,
    format: WorkdocListFormat,
    include_context: bool,
) -> Result<String> {
    match format {
        WorkdocListFormat::Table => Ok(build_question_listing_table(document)),
        WorkdocListFormat::Markdown => {
            Ok(build_question_listing_markdown(document, include_context))
        }
        WorkdocListFormat::Json => build_question_listing_json(document, include_context),
    }
}

fn build_question_listing_table(document: &WorkDocumentState) -> String {
    let mut lines = vec![
        format!("Document: {}", document.filename),
        format!(
            "Extraction: {} | Questions: {} | questionsSha256: {}",
            if document.source.extraction_mode.is_empty() {
                "-"
            } else {
                &document.source.extraction_mode
            },
            document.questions.len(),
            short_hash(&document.source.questions_sha256)
        ),
        String::new(),
        format!(
            "{:<4} {:<14} {:<10} {:<16} {}",
            "#", "Label", "Etat", "Hash", "Question"
        ),
        format!(
            "{:<4} {:<14} {:<10} {:<16} {}",
            "---", "-----", "----", "----", "--------"
        ),
    ];
    for question in &document.questions {
        lines.push(format!(
            "{:<4} {:<14} {:<10} {:<16} {}",
            question.order,
            truncate_chars(&question.label, 14),
            truncate_chars(question.status.label(), 10),
            short_hash(&question.question_hash),
            truncate_chars(&question.text, 100)
        ));
    }
    lines.join("\n")
}

fn build_question_listing_markdown(document: &WorkDocumentState, include_context: bool) -> String {
    let mut lines = vec![
        format!("# Questions extraites - {}", document.filename),
        String::new(),
        "| Métadonnée | Valeur |".to_string(),
        "|---|---|".to_string(),
        format!(
            "| Mode extraction | {} |",
            table_cell(if document.source.extraction_mode.is_empty() {
                "-"
            } else {
                &document.source.extraction_mode
            })
        ),
        format!("| Questions | {} |", document.questions.len()),
        format!(
            "| Source SHA-256 | {} |",
            table_cell(&document.source.file_sha256)
        ),
        format!(
            "| Questions SHA-256 | {} |",
            table_cell(&document.source.questions_sha256)
        ),
        String::new(),
        "| # | Label | Etat | Hash question | Question |".to_string(),
        "|---:|---|---|---|---|".to_string(),
    ];

    for question in &document.questions {
        lines.push(format!(
            "| {} | {} | {} | {} | {} |",
            question.order,
            table_cell(&question.label),
            table_cell(question.status.label()),
            table_cell(&question.question_hash),
            table_cell(&question.text)
        ));
    }

    if include_context {
        lines.push(String::new());
        lines.push("## Contextes extraits".to_string());
        for question in &document.questions {
            lines.extend([
                String::new(),
                format!("### {} - {}", question.label, question.text),
                String::new(),
                if question.context.trim().is_empty() {
                    "_Aucun contexte extrait._".to_string()
                } else {
                    question.context.trim().to_string()
                },
            ]);
        }
    }

    lines.join("\n")
}

fn build_question_listing_json(
    document: &WorkDocumentState,
    include_context: bool,
) -> Result<String> {
    let questions: Vec<serde_json::Value> = document
        .questions
        .iter()
        .map(|question| {
            let mut value = serde_json::json!({
                "order": question.order,
                "label": question.label,
                "status": question.status.label(),
                "questionHash": question.question_hash,
                "answerHash": question.answer_hash,
                "text": question.text,
                "answeredAt": question.answered_at,
                "error": question.error,
            });
            if include_context {
                value["context"] = serde_json::Value::String(question.context.clone());
            }
            value
        })
        .collect();
    let payload = serde_json::json!({
        "filename": document.filename,
        "extractionMode": document.source.extraction_mode,
        "fileSha256": document.source.file_sha256,
        "markdownSha256": document.source.markdown_sha256,
        "questionsSha256": document.source.questions_sha256,
        "questionCount": document.questions.len(),
        "questions": questions,
    });
    serde_json::to_string_pretty(&payload).context("Impossible de sérialiser la liste workdoc")
}

fn append_review_actions(lines: &mut Vec<String>, document: &WorkDocumentState) {
    let mut actions = Vec::new();
    for question in &document.questions {
        match question.status {
            WorkQuestionStatus::Pending | WorkQuestionStatus::Answering => actions.push((
                question.label.clone(),
                question.status.label().to_string(),
                "Générer la réponse Code Explorer avant diffusion.".to_string(),
            )),
            WorkQuestionStatus::Error => actions.push((
                question.label.clone(),
                question.status.label().to_string(),
                question
                    .error
                    .as_ref()
                    .map(|error| format!("Relancer la question après correction: {error}"))
                    .unwrap_or_else(|| "Relancer la question après correction.".to_string()),
            )),
            WorkQuestionStatus::Answered => {
                let answer = question.answer.as_deref().unwrap_or("");
                if count_source_references(answer) == 0 {
                    actions.push((
                        question.label.clone(),
                        question.status.label().to_string(),
                        "Ajouter des sources exactes issues du code consulté.".to_string(),
                    ));
                }
                if is_short_answer(answer) {
                    actions.push((
                        question.label.clone(),
                        question.status.label().to_string(),
                        "Enrichir la réponse avec preuves, impacts et limites.".to_string(),
                    ));
                }
            }
        }
    }

    if actions.is_empty() {
        lines.extend([
            "> [!TIP]".to_string(),
            "> Aucune action bloquante détectée. Relis les sources citées puis génère le DOCX ou le PDF final.".to_string(),
        ]);
    } else {
        lines.extend([
            "| Question | État | Action recommandée |".to_string(),
            "|---|---|---|".to_string(),
        ]);
        for (label, status, action) in actions {
            lines.push(format!(
                "| {} | {} | {} |",
                table_cell(&label),
                table_cell(&status),
                table_cell(&action)
            ));
        }
    }
}

fn append_source_index(lines: &mut Vec<String>, document: &WorkDocumentState) {
    let mut sources = BTreeMap::<String, Vec<String>>::new();
    for question in &document.questions {
        let Some(answer) = &question.answer else {
            continue;
        };
        for source in extract_source_references(answer) {
            sources
                .entry(source)
                .or_default()
                .push(question.label.clone());
        }
    }

    lines.extend(["## Index des sources citées".to_string(), String::new()]);
    if sources.is_empty() {
        lines.extend([
            "> [!WARNING]".to_string(),
            "> Aucune source détectée dans les réponses.".to_string(),
            String::new(),
        ]);
        return;
    }
    lines.extend([
        "| Fichier | Questions | Occurrences |".to_string(),
        "|---|---|---:|".to_string(),
    ]);
    for (path, labels) in sources {
        lines.push(format!(
            "| {} | {} | {} |",
            table_cell(&path),
            table_cell(&labels.join(", ")),
            labels.len()
        ));
    }
    lines.push(String::new());
}

fn append_quality_markdown(lines: &mut Vec<String>, quality: &WorkQuality) {
    lines.extend([
        "## Contrôle qualité documentaire".to_string(),
        String::new(),
        "| Contrôle | Valeur | Statut |".to_string(),
        "|---|---:|---|".to_string(),
        format!(
            "| Questions répondues | {}/{} | {} |",
            quality.answered,
            quality.total,
            if quality.answered == quality.total {
                "OK"
            } else {
                "À relire"
            }
        ),
        format!(
            "| Fichiers sources cités | {} | {} |",
            quality.source_files,
            if quality.source_files > 0 {
                "OK"
            } else {
                "À relire"
            }
        ),
        format!(
            "| Diagrammes Mermaid | {} | {} |",
            quality.diagrams,
            if quality.diagrams > 0 {
                "OK"
            } else {
                "Optionnel"
            }
        ),
        format!(
            "| Blocs de code | {} | {} |",
            quality.code_blocks,
            if quality.code_blocks > 0 {
                "OK"
            } else {
                "À enrichir"
            }
        ),
    ]);
}

fn work_document_export_title(document: &WorkDocumentState) -> String {
    let quality = WorkQuality::from_document(document);
    let readiness = quality.readiness_label();
    if readiness == "Prêt pour relecture finale" {
        format!("Livrable Code Explorer - {}", document.filename)
    } else {
        format!("Livrable Code Explorer ({readiness}) - {}", document.filename)
    }
}

fn load_state(path: &Path) -> Result<WorkDocumentState> {
    let text = fs::read_to_string(path)
        .with_context(|| format!("Impossible de lire le state {}", path.display()))?;
    let mut document: WorkDocumentState = serde_json::from_str(&text)
        .with_context(|| format!("Document de travail JSON invalide: {}", path.display()))?;
    refresh_document_hashes(&mut document, None);
    Ok(document)
}

fn write_state(path: &Path, document: &WorkDocumentState) -> Result<()> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }
    let text = serde_json::to_string_pretty(document)?;
    fs::write(path, text).with_context(|| format!("Impossible d'écrire {}", path.display()))
}

fn write_text_file(path: &Path, content: &str) -> Result<()> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, content).with_context(|| format!("Impossible d'écrire {}", path.display()))
}

fn refresh_document_hashes(document: &mut WorkDocumentState, raw_docx_sha256: Option<String>) {
    if let Some(file_sha256) = raw_docx_sha256 {
        document.source.file_sha256 = file_sha256;
    }
    document.source.markdown_sha256 = sha256_text(&normalize_hash_text(&document.source_markdown));
    document.source.question_count = document.questions.len();

    let mut question_hashes = Vec::with_capacity(document.questions.len());
    for question in &mut document.questions {
        question.question_hash = sha256_text(&stable_question_payload(question));
        if let Some(answer) = &question.answer {
            question.answer_hash = sha256_text(&normalize_hash_text(answer));
        } else {
            question.answer_hash.clear();
        }
        question_hashes.push(question.question_hash.clone());
    }
    document.source.questions_sha256 = sha256_text(&question_hashes.join("\n"));

    if document.generation.prompt_version.is_empty() {
        document.generation.prompt_version = PROMPT_VERSION.to_string();
    }
    document.generation.updated_at = Some(unix_time_ms());
}

fn stable_question_payload(question: &WorkQuestionState) -> String {
    [
        question.order.to_string(),
        normalize_hash_text(&question.label),
        normalize_hash_text(&question.text),
        normalize_hash_text(&question.context),
    ]
    .join("\n---\n")
}

fn normalize_hash_text(value: &str) -> String {
    value
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .lines()
        .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

fn update_ledger(
    state_path: &Path,
    document: &WorkDocumentState,
    repo_path: Option<&Path>,
) -> Result<PathBuf> {
    let ledger_path = ledger_path_for(
        state_path,
        repo_path.or_else(|| document_repo_path(document)),
    );
    let mut ledger = load_ledger(&ledger_path)?;
    let entry = WorkdocLedgerEntry::from_document(state_path, document);
    if let Some(existing) = ledger
        .entries
        .iter_mut()
        .find(|candidate| candidate.run_id == entry.run_id)
    {
        *existing = entry;
    } else {
        ledger.entries.push(entry);
    }
    ledger.entries.sort_by(|left, right| {
        right
            .updated_at
            .cmp(&left.updated_at)
            .then_with(|| left.run_id.cmp(&right.run_id))
    });
    if let Some(parent) = ledger_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&ledger_path, serde_json::to_string_pretty(&ledger)?)
        .with_context(|| format!("Impossible d'écrire le ledger {}", ledger_path.display()))?;
    Ok(ledger_path)
}

fn print_status(
    document: &WorkDocumentState,
    state_path: &Path,
    repo_path: Option<&Path>,
) -> Result<()> {
    let quality = WorkQuality::from_document(document);
    println!("{} {}", "Document".bold(), document.filename);
    println!(
        "  fileSha256:      {}",
        short_hash(&document.source.file_sha256)
    );
    println!(
        "  extraction:      {}",
        if document.source.extraction_mode.is_empty() {
            "-"
        } else {
            &document.source.extraction_mode
        }
    );
    println!(
        "  markdownSha256:  {}",
        short_hash(&document.source.markdown_sha256)
    );
    println!(
        "  questionsSha256: {}",
        short_hash(&document.source.questions_sha256)
    );
    println!(
        "  questions:       {} total, {} répondues, {} restantes, {} erreurs",
        quality.total, quality.answered, quality.pending, quality.errors
    );
    println!("  state:           {}", state_path.display());

    let ledger_paths = candidate_ledger_paths(
        state_path,
        repo_path.or_else(|| document_repo_path(document)),
    );
    let mut matches = Vec::new();
    for ledger_path in &ledger_paths {
        let ledger = load_ledger(ledger_path)?;
        for entry in ledger.entries {
            let same_questions = !document.source.questions_sha256.is_empty()
                && entry.questions_sha256 == document.source.questions_sha256;
            let same_source = !document.source.file_sha256.is_empty()
                && entry.source_file_sha256 == document.source.file_sha256;
            if same_questions {
                matches.push((ledger_path.clone(), entry, "questions"));
            } else if same_source {
                matches.push((ledger_path.clone(), entry, "source"));
            }
        }
    }
    matches.sort_by(|left, right| {
        let left_rank = if left.2 == "questions" { 0 } else { 1 };
        let right_rank = if right.2 == "questions" { 0 } else { 1 };
        left_rank
            .cmp(&right_rank)
            .then_with(|| right.1.updated_at.cmp(&left.1.updated_at))
    });

    if matches.is_empty() {
        println!(
            "{} Aucun run précédent correspondant trouvé.",
            "INFO".cyan()
        );
    } else {
        println!(
            "{} {} run(s) correspondant(s):",
            "OK".green(),
            matches.len()
        );
        for (ledger_path, entry, match_kind) in matches.iter().take(10) {
            println!(
                "  - {} | match={} | {}/{} réponses | state={} | docx={} | ledger={}",
                entry.updated_at,
                match_kind,
                entry.answered_count,
                entry.question_count,
                entry.state_path.as_deref().unwrap_or("-"),
                entry.docx_path.as_deref().unwrap_or("-"),
                ledger_path.display()
            );
        }
    }

    Ok(())
}

fn reuse_previous_answers(
    document: &mut WorkDocumentState,
    current_state_path: &Path,
    repo_path: Option<&Path>,
    explicit_states: &[PathBuf],
    scan_ledger: bool,
) -> Result<usize> {
    refresh_document_hashes(document, None);
    let candidates = collect_reuse_candidate_paths(
        document,
        current_state_path,
        repo_path,
        explicit_states,
        scan_ledger,
    )?;

    let mut reused_total = 0usize;
    for (candidate_path, explicit) in candidates {
        if same_path(&candidate_path, current_state_path) {
            continue;
        }
        if !candidate_path.exists() {
            if explicit {
                return Err(anyhow::anyhow!(
                    "State de réutilisation introuvable: {}",
                    candidate_path.display()
                ));
            }
            continue;
        }

        let previous = match load_state(&candidate_path) {
            Ok(previous) => previous,
            Err(err) if explicit => {
                return Err(err).with_context(|| {
                    format!(
                        "Impossible de charger le state de réutilisation {}",
                        candidate_path.display()
                    )
                });
            }
            Err(err) => {
                println!(
                    "{} State précédent ignoré ({}): {}",
                    "WARN".yellow(),
                    candidate_path.display(),
                    err
                );
                continue;
            }
        };

        if !explicit && !is_reuse_compatible(document, &previous) {
            continue;
        }

        let answers_by_hash: BTreeMap<String, (String, String, Option<u64>)> = previous
            .questions
            .iter()
            .filter(|question| question.status == WorkQuestionStatus::Answered)
            .filter_map(|question| {
                let answer = question.answer.as_ref()?.trim();
                if question.question_hash.is_empty() || answer.is_empty() {
                    return None;
                }
                Some((
                    question.question_hash.clone(),
                    (
                        question.answer.clone().unwrap_or_default(),
                        if question.answer_hash.is_empty() {
                            sha256_text(&normalize_hash_text(answer))
                        } else {
                            question.answer_hash.clone()
                        },
                        question.answered_at,
                    ),
                ))
            })
            .collect();

        if answers_by_hash.is_empty() {
            continue;
        }

        let mut reused_from_state = 0usize;
        for question in &mut document.questions {
            if question.status == WorkQuestionStatus::Answered || question.question_hash.is_empty()
            {
                continue;
            }
            if let Some((answer, answer_hash, answered_at)) =
                answers_by_hash.get(&question.question_hash)
            {
                question.status = WorkQuestionStatus::Answered;
                question.answer = Some(answer.clone());
                question.answer_hash = answer_hash.clone();
                question.answered_at = *answered_at;
                question.error = None;
                reused_from_state += 1;
            }
        }

        if reused_from_state > 0 {
            println!(
                "{} {} réponse(s) importée(s) depuis {}",
                "INFO".cyan(),
                reused_from_state,
                candidate_path.display()
            );
            reused_total += reused_from_state;
        }
    }

    Ok(reused_total)
}

fn collect_reuse_candidate_paths(
    document: &WorkDocumentState,
    current_state_path: &Path,
    repo_path: Option<&Path>,
    explicit_states: &[PathBuf],
    scan_ledger: bool,
) -> Result<Vec<(PathBuf, bool)>> {
    let mut candidates = Vec::new();
    let mut seen = BTreeSet::new();

    for explicit_state in explicit_states {
        push_reuse_candidate(&mut candidates, &mut seen, explicit_state.clone(), true);
    }

    if scan_ledger {
        let ledger_paths = candidate_ledger_paths(
            current_state_path,
            repo_path.or_else(|| document_repo_path(document)),
        );
        for ledger_path in ledger_paths {
            let ledger = load_ledger(&ledger_path)?;
            for entry in ledger.entries {
                if entry.source_file_sha256 != document.source.file_sha256
                    && entry.questions_sha256 != document.source.questions_sha256
                {
                    continue;
                }
                if let Some(state_path) = entry.state_path {
                    push_reuse_candidate(
                        &mut candidates,
                        &mut seen,
                        PathBuf::from(state_path),
                        false,
                    );
                }
            }
        }
    }

    Ok(candidates)
}

fn push_reuse_candidate(
    candidates: &mut Vec<(PathBuf, bool)>,
    seen: &mut BTreeSet<String>,
    path: PathBuf,
    explicit: bool,
) {
    let key = path_key(&path);
    if seen.insert(key) {
        candidates.push((path, explicit));
    }
}

fn is_reuse_compatible(current: &WorkDocumentState, previous: &WorkDocumentState) -> bool {
    let same_source = !current.source.file_sha256.is_empty()
        && current.source.file_sha256 == previous.source.file_sha256;
    let same_questions = !current.source.questions_sha256.is_empty()
        && current.source.questions_sha256 == previous.source.questions_sha256;
    if !same_source && !same_questions {
        return false;
    }

    match (
        current.generation.repo_index_sha256.as_deref(),
        previous.generation.repo_index_sha256.as_deref(),
    ) {
        (Some(current_hash), Some(previous_hash)) => current_hash == previous_hash,
        _ => true,
    }
}

fn load_ledger(path: &Path) -> Result<WorkdocLedger> {
    if !path.exists() {
        return Ok(WorkdocLedger::default());
    }
    let text = fs::read_to_string(path)
        .with_context(|| format!("Impossible de lire le ledger {}", path.display()))?;
    serde_json::from_str(&text)
        .with_context(|| format!("Ledger workdoc invalide: {}", path.display()))
}

fn ledger_path_for(state_path: &Path, repo_path: Option<&Path>) -> PathBuf {
    if let Some(repo_path) = repo_path {
        return repo_path
            .join(".codeexplorer")
            .join("workdocs")
            .join("ledger.json");
    }
    state_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(".codeexplorer")
        .join("workdocs")
        .join("ledger.json")
}

fn candidate_ledger_paths(state_path: &Path, repo_path: Option<&Path>) -> Vec<PathBuf> {
    let mut paths = vec![ledger_path_for(state_path, None)];
    if let Some(repo_path) = repo_path {
        let repo_ledger = ledger_path_for(state_path, Some(repo_path));
        if !paths.iter().any(|path| path == &repo_ledger) {
            paths.push(repo_ledger);
        }
    }
    paths
}

fn document_repo_path(document: &WorkDocumentState) -> Option<&Path> {
    document
        .repo
        .as_deref()
        .filter(|repo| !repo.trim().is_empty())
        .map(Path::new)
}

fn sha256_file(path: &Path) -> Result<String> {
    fs::read(path)
        .map(|bytes| sha256_bytes(&bytes))
        .with_context(|| format!("Impossible de calculer le hash {}", path.display()))
}

fn sha256_text(value: &str) -> String {
    sha256_bytes(value.as_bytes())
}

fn sha256_bytes(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn path_display(path: &Path) -> String {
    path.canonicalize()
        .unwrap_or_else(|_| path.to_path_buf())
        .display()
        .to_string()
}

fn path_key(path: &Path) -> String {
    path_display(path).to_ascii_lowercase()
}

fn same_path(left: &Path, right: &Path) -> bool {
    path_key(left) == path_key(right)
}

fn short_hash(hash: &str) -> &str {
    if hash.len() > 16 {
        &hash[..16]
    } else if hash.is_empty() {
        "-"
    } else {
        hash
    }
}

fn is_json_path(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("json"))
}

fn print_extract_summary(state_path: &Path, document: &WorkDocumentState) {
    println!(
        "{} {} question(s) extraites depuis {} (mode {})",
        "OK".green(),
        document.questions.len(),
        document.filename,
        if document.source.extraction_mode.is_empty() {
            "-"
        } else {
            &document.source.extraction_mode
        }
    );
    println!("{} State: {}", "OK".green(), state_path.display());
}

fn default_state_path(docx: &Path) -> PathBuf {
    docx.with_extension("workdoc.json")
}

fn default_docx_output_path(input: &Path) -> PathBuf {
    let stem = input
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("document-travail");
    input.with_file_name(format!("{stem}-answered.docx"))
}

fn unix_time_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or_default()
}

fn table_cell(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('|', "\\|")
        .replace('\n', "<br>")
        .trim()
        .to_string()
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    let normalized = value.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut chars = normalized.chars();
    let mut truncated: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        truncated.push_str("...");
    }
    truncated
}

fn normalize_answer(answer: &str) -> String {
    answer
        .replace("<true>", "`true`")
        .replace("<false>", "`false`")
        .replace("<null>", "`null`")
        .replace("-->||", "-->")
        .trim()
        .to_string()
}

fn count_mermaid_diagrams(answer: &str) -> usize {
    answer.matches("```mermaid").count()
}

fn count_code_blocks(answer: &str) -> usize {
    answer
        .matches("```")
        .count()
        .saturating_sub(count_mermaid_diagrams(answer))
}

fn count_source_references(answer: &str) -> usize {
    extract_source_references(answer).len()
}

fn extract_source_references(answer: &str) -> Vec<String> {
    answer
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim().trim_start_matches(['-', '*']).trim();
            if !looks_like_source_path(trimmed) {
                return None;
            }
            Some(
                trimmed
                    .trim_matches('`')
                    .split_whitespace()
                    .next()
                    .unwrap_or(trimmed)
                    .trim_matches(|c| c == ',' || c == ';')
                    .to_string(),
            )
        })
        .filter(|path| looks_like_source_path(path))
        .collect()
}

fn looks_like_source_path(value: &str) -> bool {
    let lower = value.trim_matches('`').to_ascii_lowercase();
    (lower.contains('/') || lower.contains('\\'))
        && [
            ".cs", ".ts", ".tsx", ".js", ".rs", ".sql", ".cshtml", ".xml", ".json", ".md",
        ]
        .iter()
        .any(|ext| lower.contains(ext))
}

fn is_short_answer(answer: &str) -> bool {
    answer.split_whitespace().count() < 80
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WorkDocumentState {
    id: String,
    filename: String,
    imported_at: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    repo: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    repo_name: Option<String>,
    source_bytes: usize,
    markdown_chars: usize,
    source_markdown: String,
    questions: Vec<WorkQuestionState>,
    #[serde(default)]
    source: WorkSourceHashes,
    #[serde(default)]
    generation: WorkGenerationMetadata,
    #[serde(default)]
    outputs: WorkOutputs,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WorkQuestionState {
    id: String,
    order: usize,
    label: String,
    text: String,
    context: String,
    status: WorkQuestionStatus,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    question_hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    answer: Option<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    answer_hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    answered_at: Option<u64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WorkSourceHashes {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    extraction_mode: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    file_sha256: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    markdown_sha256: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    questions_sha256: String,
    #[serde(default, skip_serializing_if = "is_zero")]
    question_count: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WorkGenerationMetadata {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    prompt_version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    reasoning_effort: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    repo_index_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    updated_at: Option<u64>,
}

impl WorkGenerationMetadata {
    fn from_context(repo_path: Option<&Path>) -> Self {
        let config = super::generate::load_llm_config();
        Self {
            prompt_version: PROMPT_VERSION.to_string(),
            provider: config.as_ref().map(|config| config.provider.clone()),
            model: config.as_ref().map(|config| config.model.clone()),
            reasoning_effort: config
                .as_ref()
                .map(|config| config.reasoning_effort.clone())
                .filter(|effort| !effort.trim().is_empty()),
            repo_index_sha256: repo_path.and_then(|path| {
                let graph = path.join(".codeexplorer").join("graph.bin");
                graph.exists().then(|| sha256_file(&graph).ok()).flatten()
            }),
            updated_at: Some(unix_time_ms()),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WorkOutputs {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    markdown_path: Option<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    markdown_sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    docx_path: Option<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    docx_sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    exported_at: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WorkdocLedger {
    version: u32,
    entries: Vec<WorkdocLedgerEntry>,
}

impl Default for WorkdocLedger {
    fn default() -> Self {
        Self {
            version: LEDGER_VERSION,
            entries: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WorkdocLedgerEntry {
    run_id: String,
    filename: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    repo: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    repo_name: Option<String>,
    source_file_sha256: String,
    markdown_sha256: String,
    questions_sha256: String,
    question_count: usize,
    answered_count: usize,
    error_count: usize,
    prompt_version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    repo_index_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    state_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    markdown_path: Option<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    output_markdown_sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    docx_path: Option<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    output_docx_sha256: String,
    updated_at: u64,
}

impl WorkdocLedgerEntry {
    fn from_document(state_path: &Path, document: &WorkDocumentState) -> Self {
        let quality = WorkQuality::from_document(document);
        Self {
            run_id: document.id.clone(),
            filename: document.filename.clone(),
            repo: document.repo.clone(),
            repo_name: document.repo_name.clone(),
            source_file_sha256: document.source.file_sha256.clone(),
            markdown_sha256: document.source.markdown_sha256.clone(),
            questions_sha256: document.source.questions_sha256.clone(),
            question_count: quality.total,
            answered_count: quality.answered,
            error_count: quality.errors,
            prompt_version: document.generation.prompt_version.clone(),
            provider: document.generation.provider.clone(),
            model: document.generation.model.clone(),
            repo_index_sha256: document.generation.repo_index_sha256.clone(),
            state_path: Some(path_display(state_path)),
            markdown_path: document.outputs.markdown_path.clone(),
            output_markdown_sha256: document.outputs.markdown_sha256.clone(),
            docx_path: document.outputs.docx_path.clone(),
            output_docx_sha256: document.outputs.docx_sha256.clone(),
            updated_at: document
                .outputs
                .exported_at
                .or(document.generation.updated_at)
                .unwrap_or_else(unix_time_ms),
        }
    }
}

fn is_zero(value: &usize) -> bool {
    *value == 0
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum WorkQuestionStatus {
    Pending,
    Answering,
    Answered,
    Error,
}

impl WorkQuestionStatus {
    fn label(self) -> &'static str {
        match self {
            Self::Pending => "À traiter",
            Self::Answering => "En cours",
            Self::Answered => "Répondue",
            Self::Error => "Erreur",
        }
    }
}

struct WorkQuality {
    total: usize,
    answered: usize,
    errors: usize,
    pending: usize,
    source_files: usize,
    diagrams: usize,
    code_blocks: usize,
}

impl WorkQuality {
    fn from_document(document: &WorkDocumentState) -> Self {
        let answered_questions: Vec<&WorkQuestionState> = document
            .questions
            .iter()
            .filter(|question| question.status == WorkQuestionStatus::Answered)
            .collect();
        let mut source_counts = BTreeMap::new();
        for question in &answered_questions {
            for source in extract_source_references(question.answer.as_deref().unwrap_or("")) {
                *source_counts.entry(source).or_insert(0usize) += 1;
            }
        }
        Self {
            total: document.questions.len(),
            answered: answered_questions.len(),
            errors: document
                .questions
                .iter()
                .filter(|question| question.status == WorkQuestionStatus::Error)
                .count(),
            pending: document
                .questions
                .iter()
                .filter(|question| {
                    matches!(
                        question.status,
                        WorkQuestionStatus::Pending | WorkQuestionStatus::Answering
                    )
                })
                .count(),
            source_files: source_counts.len(),
            diagrams: answered_questions
                .iter()
                .map(|question| count_mermaid_diagrams(question.answer.as_deref().unwrap_or("")))
                .sum(),
            code_blocks: answered_questions
                .iter()
                .map(|question| count_code_blocks(question.answer.as_deref().unwrap_or("")))
                .sum(),
        }
    }

    fn readiness_label(&self) -> String {
        if self.total == 0 || self.answered == 0 {
            "Brouillon bloqué".to_string()
        } else if self.pending == 0 && self.errors == 0 {
            "Prêt pour relecture finale".to_string()
        } else {
            "Brouillon à relire".to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_state() -> WorkDocumentState {
        WorkDocumentState {
            id: "doc-1".to_string(),
            filename: "Questions Sample.docx".to_string(),
            imported_at: 1,
            repo: Some("D:/taf/sample-app".to_string()),
            repo_name: Some("sample-app".to_string()),
            source_bytes: 10,
            markdown_chars: 20,
            source_markdown: "# Source".to_string(),
            questions: vec![WorkQuestionState {
                id: "q-001".to_string(),
                order: 1,
                label: "Q1".to_string(),
                text: "Comment fonctionne le calcul ?".to_string(),
                context: "Contexte".to_string(),
                status: WorkQuestionStatus::Answered,
                question_hash: String::new(),
                answer: Some(
                    "Réponse détaillée avec preuve.\n\n```csharp\npublic void Run() {}\n```\n\n## Sources\n- Acme.Sample/Calcul/Service.cs"
                        .to_string(),
                ),
                answer_hash: String::new(),
                error: None,
                answered_at: Some(2),
            }],
            source: WorkSourceHashes::default(),
            generation: WorkGenerationMetadata::default(),
            outputs: WorkOutputs::default(),
        }
    }

    #[test]
    fn markdown_export_keeps_workdoc_cover_metrics() {
        let markdown = build_work_document_markdown(&sample_state());

        assert!(markdown.contains("| Questions extraites | 1 |"));
        assert!(markdown.contains("| Questions répondues | 1 |"));
        assert!(markdown.contains("| Statut du livrable | Prêt pour relecture finale |"));
        assert!(markdown.contains("### Chapitre 1 - Q1"));
        assert!(markdown.contains("Acme.Sample/Calcul/Service.cs"));
    }

    #[test]
    fn source_reference_detection_finds_code_paths() {
        let answer = "## Sources\n- `Acme.Sample/Views/Home.cshtml`\n- src/main.rs";
        let sources = extract_source_references(answer);

        assert_eq!(sources.len(), 2);
        assert!(sources.contains(&"Acme.Sample/Views/Home.cshtml".to_string()));
        assert!(sources.contains(&"src/main.rs".to_string()));
    }

    #[test]
    fn work_question_prompt_preserves_specific_document_requests() {
        let document = sample_state();
        let prompt = build_work_question_prompt(&document, &document.questions[0], "sample-app");

        assert!(prompt.contains("Si la question demande une \"section à part\""));
        assert!(prompt.contains("ajoute une cartographie par canal"));
        assert!(prompt.contains("fournis au moins ce nombre d’exemples concrets"));
        assert!(prompt.contains("produis une matrice de cas"));
    }

    #[test]
    fn refresh_document_hashes_populates_source_question_and_answer_hashes() {
        let mut document = sample_state();

        refresh_document_hashes(&mut document, Some("raw-docx".to_string()));

        assert_eq!(document.source.file_sha256, "raw-docx");
        assert_eq!(document.source.question_count, 1);
        assert_eq!(document.source.markdown_sha256.len(), 64);
        assert_eq!(document.source.questions_sha256.len(), 64);
        assert_eq!(document.questions[0].question_hash.len(), 64);
        assert_eq!(document.questions[0].answer_hash.len(), 64);
        assert_eq!(document.generation.prompt_version, PROMPT_VERSION);
    }

    #[test]
    fn auto_extraction_prefers_numbered_question_headings() {
        let markdown = r#"
# Ancien sommaire
Q1.1 — Ancienne question historique
Q2.1 — Autre question historique

# Question 1 : Première question active ?
### Réponse détaillée proposée
Ancienne réponse à ignorer pour le libellé.

# Question 2 : Cette section n'est pas claire. Serait-ce possible d'avoir :
- un exemple concret ;
- le détail des calculs.
### Réponse détaillée proposée
Ancienne réponse.

# Question 3 : Dernière question active ?
"#;

        let extraction = extract_workdoc_questions_for_cli(markdown, WorkdocExtractMode::Auto);

        assert_eq!(extraction.mode, WorkdocExtractMode::Headings);
        assert_eq!(extraction.questions.len(), 3);
        assert_eq!(extraction.questions[0].label, "Question 1");
        assert!(extraction.questions[1].text.contains("un exemple concret"));
        assert!(!extraction.questions[1].text.contains("Ancienne réponse"));
    }

    #[test]
    fn heading_extraction_merges_repeated_question_numbers() {
        let markdown = r#"
# Question 9 : Cette section n'est pas claire.
### Réponse
Bloc A.

# Question 9 : Où se trouve le service métier ?
### Réponse
Bloc B.
"#;

        let extraction = extract_workdoc_questions_for_cli(markdown, WorkdocExtractMode::Headings);

        assert_eq!(extraction.questions.len(), 1);
        assert_eq!(extraction.questions[0].label, "Question 9");
        assert!(extraction.questions[0]
            .text
            .contains("Où se trouve le service métier"));
        assert!(extraction.questions[0].context.contains("Bloc A"));
        assert!(extraction.questions[0].context.contains("Bloc B"));
    }

    #[test]
    fn question_listing_exports_markdown_and_json_manifest() {
        let mut document = sample_state();
        document.source.extraction_mode = "headings".to_string();
        refresh_document_hashes(&mut document, Some("raw-docx".to_string()));

        let markdown = build_question_listing(&document, WorkdocListFormat::Markdown, true)
            .expect("markdown listing should build");
        assert!(markdown.contains("# Questions extraites - Questions Sample.docx"));
        assert!(markdown.contains("| Questions | 1 |"));
        assert!(markdown.contains("## Contextes extraits"));
        assert!(markdown.contains("Comment fonctionne le calcul ?"));

        let json = build_question_listing(&document, WorkdocListFormat::Json, false)
            .expect("json listing should build");
        assert!(json.contains("\"questionCount\": 1"));
        assert!(json.contains("\"extractionMode\": \"headings\""));
        assert!(!json.contains("\"context\""));
    }

    #[test]
    fn workdoc_ledger_round_trips_generation_metadata() {
        let root = std::env::temp_dir().join(format!(
            "code-explorer-workdoc-ledger-test-{}",
            uuid::Uuid::new_v4()
        ));
        let repo_path = root.join("repo");
        let state_path = root.join("state").join("sample.workdoc.json");
        let mut document = sample_state();
        document.repo = Some(path_display(&repo_path));
        refresh_document_hashes(&mut document, Some("raw-docx".to_string()));
        document.outputs.markdown_path = Some(root.join("answers.md").display().to_string());
        document.outputs.markdown_sha256 = sha256_text("markdown");
        document.outputs.docx_path = Some(root.join("answers.docx").display().to_string());
        document.outputs.docx_sha256 = sha256_text("docx");
        document.outputs.exported_at = Some(42);

        let ledger_path =
            update_ledger(&state_path, &document, Some(&repo_path)).expect("ledger should write");
        let ledger = load_ledger(&ledger_path).expect("ledger should load");

        assert_eq!(ledger.version, LEDGER_VERSION);
        assert_eq!(ledger.entries.len(), 1);
        assert_eq!(ledger.entries[0].run_id, "doc-1");
        assert_eq!(ledger.entries[0].source_file_sha256, "raw-docx");
        assert_eq!(ledger.entries[0].answered_count, 1);
        assert_eq!(ledger.entries[0].output_docx_sha256, sha256_text("docx"));

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn reuse_previous_answers_imports_explicit_state_by_question_hash() {
        let root = std::env::temp_dir().join(format!(
            "code-explorer-workdoc-reuse-explicit-test-{}",
            uuid::Uuid::new_v4()
        ));
        let previous_path = root.join("previous.workdoc.json");
        let current_path = root.join("current.workdoc.json");

        let mut previous = sample_state();
        refresh_document_hashes(&mut previous, Some("raw-docx".to_string()));
        write_state(&previous_path, &previous).expect("previous state should write");

        let mut current = sample_state();
        current.id = "doc-2".to_string();
        current.questions[0].status = WorkQuestionStatus::Pending;
        current.questions[0].answer = None;
        current.questions[0].answer_hash.clear();
        current.questions[0].answered_at = None;
        refresh_document_hashes(&mut current, Some("raw-docx".to_string()));

        let reused = reuse_previous_answers(
            &mut current,
            &current_path,
            None,
            std::slice::from_ref(&previous_path),
            false,
        )
        .expect("answers should be reusable");

        assert_eq!(reused, 1);
        assert_eq!(current.questions[0].status, WorkQuestionStatus::Answered);
        assert!(current.questions[0]
            .answer
            .as_deref()
            .unwrap_or_default()
            .contains("Réponse détaillée"));

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn reuse_previous_answers_scans_compatible_ledger() {
        let root = std::env::temp_dir().join(format!(
            "code-explorer-workdoc-reuse-ledger-test-{}",
            uuid::Uuid::new_v4()
        ));
        let repo_path = root.join("repo");
        let previous_path = root.join("previous.workdoc.json");
        let current_path = root.join("current.workdoc.json");

        let mut previous = sample_state();
        previous.repo = Some(path_display(&repo_path));
        previous.generation.repo_index_sha256 = Some("same-index".to_string());
        refresh_document_hashes(&mut previous, Some("raw-docx".to_string()));
        write_state(&previous_path, &previous).expect("previous state should write");
        update_ledger(&previous_path, &previous, Some(&repo_path)).expect("ledger should write");

        let mut current = sample_state();
        current.id = "doc-2".to_string();
        current.repo = Some(path_display(&repo_path));
        current.generation.repo_index_sha256 = Some("same-index".to_string());
        current.questions[0].status = WorkQuestionStatus::Pending;
        current.questions[0].answer = None;
        refresh_document_hashes(&mut current, Some("raw-docx".to_string()));

        let reused =
            reuse_previous_answers(&mut current, &current_path, Some(&repo_path), &[], true)
                .expect("ledger answers should be reusable");

        assert_eq!(reused, 1);
        assert_eq!(current.questions[0].status, WorkQuestionStatus::Answered);

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn reuse_previous_answers_skips_stale_repo_index_from_ledger() {
        let root = std::env::temp_dir().join(format!(
            "code-explorer-workdoc-reuse-stale-ledger-test-{}",
            uuid::Uuid::new_v4()
        ));
        let repo_path = root.join("repo");
        let previous_path = root.join("previous.workdoc.json");
        let current_path = root.join("current.workdoc.json");

        let mut previous = sample_state();
        previous.repo = Some(path_display(&repo_path));
        previous.generation.repo_index_sha256 = Some("old-index".to_string());
        refresh_document_hashes(&mut previous, Some("raw-docx".to_string()));
        write_state(&previous_path, &previous).expect("previous state should write");
        update_ledger(&previous_path, &previous, Some(&repo_path)).expect("ledger should write");

        let mut current = sample_state();
        current.id = "doc-2".to_string();
        current.repo = Some(path_display(&repo_path));
        current.generation.repo_index_sha256 = Some("new-index".to_string());
        current.questions[0].status = WorkQuestionStatus::Pending;
        current.questions[0].answer = None;
        refresh_document_hashes(&mut current, Some("raw-docx".to_string()));

        let reused =
            reuse_previous_answers(&mut current, &current_path, Some(&repo_path), &[], true)
                .expect("stale ledger should be skipped cleanly");

        assert_eq!(reused, 0);
        assert_eq!(current.questions[0].status, WorkQuestionStatus::Pending);

        let _ = std::fs::remove_dir_all(root);
    }
}

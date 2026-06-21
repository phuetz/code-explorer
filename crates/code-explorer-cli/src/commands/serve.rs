//! The `serve` command: starts an HTTP server for the web UI and MCP HTTP endpoint.
//!
//! `/api/chat` — Server-Sent Events (SSE) chat endpoint.
//!
//! Request body (JSON):
//! ```json
//! {
//!   "question": "Explain the DossiersController",
//!   "repo": "sample-app",
//!   "history": [
//!     { "role": "user",      "content": "Previous question" },
//!     { "role": "assistant", "content": "Previous answer"   }
//!   ]
//! }
//! ```
//!
//! Response: SSE stream of text deltas, terminated by `data: [DONE]`.

use std::collections::{HashMap, HashSet};
use std::net::IpAddr;
use std::sync::Arc;

use axum::{
    body::{Body, Bytes},
    extract::{DefaultBodyLimit, Path as AxumPath, Query},
    extract::{Request, State},
    http::{header, HeaderName, Method, StatusCode, Uri},
    middleware::{self, Next},
    response::sse::{Event, KeepAlive},
    response::Response,
    response::Sse,
    response::{IntoResponse, Redirect},
    routing::{get, post},
    Json, Router,
};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::convert::Infallible;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::services::ServeDir;

use code_explorer_core::storage::repo_manager::{registry_entry_id, RegistryEntry};
use code_explorer_mcp::backend::local::LocalBackend;
use code_explorer_mcp::transport::http::{mcp_http_router, SharedBackend};

const MAX_CHAT_QUESTION_CHARS: usize = 16_000;
const MAX_CHAT_HISTORY_MESSAGES: usize = 12;
const MAX_CHAT_HISTORY_CONTENT_CHARS: usize = 16_000;
const MAX_CHAT_HISTORY_TOTAL_CHARS: usize = 48_000;
const MAX_WORKDOC_BYTES: usize = 25 * 1024 * 1024;
const MAX_WORKDOC_EXPORT_MARKDOWN_BYTES: usize = 8 * 1024 * 1024;
const MAX_WORKDOC_STATE_BYTES: usize = 16 * 1024 * 1024;
const MAX_WORKDOC_QUESTIONS: usize = 120;
const WORKDOC_CONTEXT_CHARS: usize = 1_400;
const MAX_WORKDOC_STATE_LIST: i64 = 50;

#[derive(Deserialize)]
struct ChatRequest {
    question: String,
    #[serde(default)]
    repo: String,
    /// Optional conversation history for multi-turn context.
    /// Each entry: { "role": "user"|"assistant", "content": "..." }
    #[serde(default)]
    history: Vec<HistoryEntry>,
}

#[derive(Deserialize, Clone)]
struct HistoryEntry {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct WorkdocExtractQuery {
    #[serde(default, alias = "fileName")]
    file_name: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WorkdocExportRequest {
    #[serde(default)]
    filename: Option<String>,
    #[serde(default)]
    title: Option<String>,
    markdown: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WorkdocStateRequest {
    document: Value,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct WorkdocStateResponse {
    id: String,
    saved_at_unix_ms: u64,
    document: Value,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct WorkdocStateSummary {
    id: String,
    filename: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    repo: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    repo_name: Option<String>,
    imported_at: u64,
    saved_at_unix_ms: u64,
    question_count: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct WorkdocStateListResponse {
    documents: Vec<WorkdocStateSummary>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct WorkdocDeleteResponse {
    id: String,
    deleted: bool,
}

#[derive(Debug)]
struct WorkdocStateMetadata {
    id: String,
    filename: String,
    repo: Option<String>,
    repo_name: Option<String>,
    imported_at: u64,
    question_count: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WorkdocExtractResponse {
    document: WorkdocDocumentSummary,
    questions: Vec<WorkdocQuestion>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    question_groups: Vec<WorkdocQuestionGroup>,
    source_markdown: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WorkdocDocumentSummary {
    filename: String,
    bytes: usize,
    markdown_chars: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WorkdocQuestion {
    pub(crate) id: String,
    pub(crate) order: usize,
    pub(crate) label: String,
    pub(crate) text: String,
    pub(crate) context: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct WorkdocQuestionGroup {
    id: String,
    label: String,
    color: WorkdocQuestionGroupColor,
    question_count: usize,
    question_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct WorkdocQuestionGroupColor {
    family: String,
    label: String,
    value: String,
}

fn http_auth_token() -> Option<Arc<String>> {
    std::env::var("CODE_EXPLORER_HTTP_TOKEN")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .map(Arc::new)
}

fn is_loopback_host(host: &str) -> bool {
    let normalized = host.trim().trim_start_matches('[').trim_end_matches(']');
    normalized.eq_ignore_ascii_case("localhost")
        || normalized
            .parse::<IpAddr>()
            .map(|ip| ip.is_loopback())
            .unwrap_or(false)
}

fn is_loopback_origin(origin: &str) -> bool {
    let Ok(uri) = origin.parse::<Uri>() else {
        return false;
    };

    if !matches!(uri.scheme_str(), Some("http" | "https")) {
        return false;
    }
    if uri
        .path_and_query()
        .map(|path_and_query| path_and_query.as_str() != "/")
        .unwrap_or(false)
    {
        return false;
    }

    let Some(authority) = uri.authority() else {
        return false;
    };
    if authority.as_str().contains('@') {
        return false;
    }

    uri.host().map(is_loopback_host).unwrap_or(false)
}

fn validate_chat_payload(payload: &ChatRequest) -> Result<(), (StatusCode, String)> {
    let question_chars = payload.question.chars().count();
    if payload.question.trim().is_empty() {
        return Err((StatusCode::BAD_REQUEST, "Question is required.".to_string()));
    }
    if question_chars > MAX_CHAT_QUESTION_CHARS {
        return Err((
            StatusCode::PAYLOAD_TOO_LARGE,
            format!(
                "Question is too large ({} chars, max {}).",
                question_chars, MAX_CHAT_QUESTION_CHARS
            ),
        ));
    }
    if payload.history.len() > MAX_CHAT_HISTORY_MESSAGES {
        return Err((
            StatusCode::PAYLOAD_TOO_LARGE,
            format!(
                "Chat history is too large ({} messages, max {}).",
                payload.history.len(),
                MAX_CHAT_HISTORY_MESSAGES
            ),
        ));
    }

    let mut total_history_chars = 0usize;
    for entry in &payload.history {
        if entry.role != "user" && entry.role != "assistant" {
            return Err((
                StatusCode::BAD_REQUEST,
                "History role must be either 'user' or 'assistant'.".to_string(),
            ));
        }
        let entry_chars = entry.content.chars().count();
        if entry_chars > MAX_CHAT_HISTORY_CONTENT_CHARS {
            return Err((
                StatusCode::PAYLOAD_TOO_LARGE,
                format!(
                    "A chat history message is too large ({} chars, max {}).",
                    entry_chars, MAX_CHAT_HISTORY_CONTENT_CHARS
                ),
            ));
        }
        total_history_chars = total_history_chars.saturating_add(entry_chars);
    }

    if total_history_chars > MAX_CHAT_HISTORY_TOTAL_CHARS {
        return Err((
            StatusCode::PAYLOAD_TOO_LARGE,
            format!(
                "Chat history is too large ({} chars, max {}).",
                total_history_chars, MAX_CHAT_HISTORY_TOTAL_CHARS
            ),
        ));
    }

    Ok(())
}

async fn auth_middleware(
    State(token): State<Arc<String>>,
    request: Request,
    next: Next,
) -> Response {
    let authorized = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .map(|value| value == token.as_str())
        .unwrap_or(false)
        || request
            .headers()
            .get("x-api-key")
            .and_then(|value| value.to_str().ok())
            .map(|value| value == token.as_str())
            .unwrap_or(false);

    if !authorized {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "Missing or invalid HTTP auth token" })),
        )
            .into_response();
    }

    next.run(request).await
}

pub async fn run(port: u16, host: &str) -> anyhow::Result<()> {
    let auth_token = http_auth_token();
    if auth_token.is_none() {
        if is_loopback_host(host) {
            eprintln!(
                "Warning: CODE_EXPLORER_HTTP_TOKEN is not set; HTTP APIs are only intended for this local machine."
            );
        } else {
            anyhow::bail!(
                "Refusing to start HTTP server on non-loopback host '{host}' without CODE_EXPLORER_HTTP_TOKEN. Set CODE_EXPLORER_HTTP_TOKEN or bind to 127.0.0.1."
            );
        }
    }

    let mut backend = LocalBackend::new();
    if let Err(e) = backend.init() {
        eprintln!("Warning: failed to initialize backend: {e}");
    }

    let shared: SharedBackend = Arc::new(Mutex::new(backend));

    // CORS -- allow browser access from bundled UI and loopback dev servers,
    // including custom ChatPort values chosen by the launcher.
    let cors = CorsLayer::new()
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers([
            header::CONTENT_TYPE,
            header::ACCEPT,
            header::AUTHORIZATION,
            HeaderName::from_static("x-api-key"),
        ])
        .allow_origin(AllowOrigin::predicate(|origin, _request_parts| {
            origin.to_str().map(is_loopback_origin).unwrap_or(false)
        }));

    let chat_routes = Router::new()
        .route("/api/chat", post(chat_handler).get(chat_get_redirect))
        .route(
            "/api/workdocs/extract",
            post(workdoc_extract_handler).layer(DefaultBodyLimit::max(MAX_WORKDOC_BYTES)),
        )
        .route(
            "/api/workdocs/export/docx",
            post(workdoc_export_docx_handler).layer(DefaultBodyLimit::max(
                MAX_WORKDOC_EXPORT_MARKDOWN_BYTES + 4096,
            )),
        )
        .route(
            "/api/workdocs/state",
            get(workdoc_list_state_handler)
                .post(workdoc_save_state_handler)
                .put(workdoc_save_state_handler)
                .layer(DefaultBodyLimit::max(MAX_WORKDOC_STATE_BYTES + 4096)),
        )
        .route(
            "/api/workdocs/state/:id",
            get(workdoc_get_state_handler).delete(workdoc_delete_state_handler),
        );
    let chat_routes = if let Some(token) = auth_token {
        chat_routes.route_layer(middleware::from_fn_with_state(token, auth_middleware))
    } else {
        chat_routes
    };

    // Base router from MCP, plus the chat endpoint using the same optional
    // bearer token gate (`CODE_EXPLORER_HTTP_TOKEN`) as the MCP HTTP routes.
    let app = mcp_http_router().merge(chat_routes).layer(cors);

    // Static file serving — two candidates, first match wins.
    //
    // 1. `<binary_dir>/web/` — used by the portable USB kit. The packaging
    //    script copies `chat-ui/dist/` here so visiting the server root
    //    in a browser loads the React UI directly, no `npm run dev` required.
    // 2. `<cwd>/.codeexplorer/docs/` — legacy: the generated documentation HTML
    //    of whatever repo `code-explorer serve` was started in.
    let bin_web_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("web")))
        .filter(|d| d.exists());
    let cwd_docs_dir = std::env::current_dir()?.join(".codeexplorer").join("docs");

    let app = if let Some(web_dir) = bin_web_dir {
        println!("Serving chat-ui from {}", web_dir.display());
        app.fallback_service(ServeDir::new(web_dir))
    } else if cwd_docs_dir.exists() {
        println!("Serving documentation from {}", cwd_docs_dir.display());
        app.fallback_service(ServeDir::new(cwd_docs_dir))
    } else {
        app
    };

    let app = app.with_state(shared);

    let addr = format!("{host}:{port}");
    println!("Code Explorer HTTP server starting on http://{addr}");
    println!("  Documentation: http://{addr}/index.html");
    println!("  Chat API:      POST http://{addr}/api/chat  (SSE)");
    println!("  MCP endpoint:  POST http://{addr}/mcp");
    println!("  Press Ctrl+C to stop");

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    println!("Server stopped.");
    Ok(())
}

async fn chat_handler(
    State(backend): State<SharedBackend>,
    Json(payload): Json<ChatRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    validate_chat_payload(&payload)?;

    let backend_guard = backend.lock().await;

    // Resolve repo path from the selected project. The React UI may send a
    // public registry id, while older callers send the display name.
    let registry = backend_guard.registry();
    let repo_entry = resolve_chat_repo_entry(&backend_guard, &payload.repo, registry)?;
    let repo_path = std::path::PathBuf::from(repo_entry.path.clone());
    let repo_tool_label = registry_entry_id(repo_entry);
    drop(backend_guard);

    let question = payload.question;
    let history = payload.history;

    // Build prior turn messages for context window (last 6 turns = 12 messages)
    let history_context: String = history
        .iter()
        .rev()
        .take(6)
        .rev()
        .map(|h| format!("**{}**: {}", h.role, h.content))
        .collect::<Vec<_>>()
        .join("\n\n");
    let augmented_question = if history_context.is_empty() {
        question
    } else {
        format!(
            "{}\n\n---\n*Contexte de la conversation précédente :*\n{}",
            question, history_context
        )
    };

    // Channel feeds the SSE stream. The tool-loop runs in a tokio::spawn
    // (no spawn_blocking — ask_question_with_tools is fully async) and the
    // callback bridges StreamEvent → typed SSE Event.
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let tx_cb = tx.clone();
    let backend_for_loop = backend.clone();

    tokio::spawn(async move {
        let stream_cb = Box::new(move |ev: super::ask::StreamEvent| {
            let event = match ev {
                super::ask::StreamEvent::Delta(text) => Event::default().data(text),
                super::ask::StreamEvent::ToolCallStart { id, name, args } => {
                    Event::default().event("tool_call").data(
                        serde_json::json!({
                            "phase": "start",
                            "id": id,
                            "name": name,
                            "args": args,
                        })
                        .to_string(),
                    )
                }
                super::ask::StreamEvent::ToolCallEnd { id, name, success } => {
                    Event::default().event("tool_call").data(
                        serde_json::json!({
                            "phase": "end",
                            "id": id,
                            "name": name,
                            "success": success,
                        })
                        .to_string(),
                    )
                }
            };
            let _ = tx_cb.send(Ok::<Event, Infallible>(event));
        });

        let result = super::ask::ask_question_with_tools(
            &augmented_question,
            &repo_path,
            backend_for_loop,
            Some(&repo_tool_label),
            Some(stream_cb),
        )
        .await;

        match result {
            Ok(_) => {
                let _ = tx.send(Ok(Event::default().data("[DONE]")));
            }
            Err(e) => {
                let _ = tx.send(Ok(Event::default()
                    .event("error")
                    .data(format!("Error: {}", e))));
                let _ = tx.send(Ok(Event::default().data("[DONE]")));
            }
        }
    });

    let stream = tokio_stream::wrappers::UnboundedReceiverStream::new(rx);
    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

async fn workdoc_extract_handler(
    Query(query): Query<WorkdocExtractQuery>,
    body: Bytes,
) -> Result<Json<WorkdocExtractResponse>, (StatusCode, String)> {
    if body.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "Le document de travail est vide.".to_string(),
        ));
    }
    if body.len() > MAX_WORKDOC_BYTES {
        return Err((
            StatusCode::PAYLOAD_TOO_LARGE,
            format!(
                "Le document est trop volumineux ({} octets, max {}).",
                body.len(),
                MAX_WORKDOC_BYTES
            ),
        ));
    }

    let filename = sanitize_workdoc_filename(query.file_name.as_deref().unwrap_or("document.docx"));
    if !filename.to_ascii_lowercase().ends_with(".docx") {
        return Err((
            StatusCode::BAD_REQUEST,
            "Seuls les documents .docx sont supportés pour l'instant.".to_string(),
        ));
    }

    let temp_path = write_workdoc_temp_file(&body)
        .await
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err))?;
    let markdown = code_explorer_rag::docx::docx_to_markdown_with_images(&temp_path).map_err(|err| {
        (
            StatusCode::BAD_REQUEST,
            format!("Impossible de lire le document DOCX: {err}"),
        )
    });
    let colored_paragraphs =
        code_explorer_rag::docx::docx_colored_paragraphs(&temp_path).unwrap_or_default();
    let _ = tokio::fs::remove_file(&temp_path).await;
    let markdown = markdown?;

    let questions = extract_workdoc_questions(&markdown);
    let question_groups = collect_workdoc_question_groups(&questions, &colored_paragraphs);
    Ok(Json(WorkdocExtractResponse {
        document: WorkdocDocumentSummary {
            filename,
            bytes: body.len(),
            markdown_chars: markdown.chars().count(),
        },
        questions,
        question_groups,
        source_markdown: markdown,
    }))
}

async fn workdoc_export_docx_handler(
    Json(payload): Json<WorkdocExportRequest>,
) -> Result<Response, (StatusCode, String)> {
    validate_workdoc_export_payload(&payload)?;

    let filename = sanitize_workdoc_export_filename(payload.filename.as_deref(), "docx");
    let title = payload
        .title
        .as_deref()
        .map(str::trim)
        .filter(|title| !title.is_empty())
        .unwrap_or("Réponses Code Explorer");
    let temp_path = workdoc_temp_output_path("docx")
        .await
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err))?;

    let markdown = payload.markdown;
    let title = title.to_string();
    let export_path = temp_path.clone();
    let export_result = tokio::task::spawn_blocking(move || {
        super::export_docx::export_markdown_as_docx(&markdown, &export_path, &title)
    })
    .await
    .map_err(|err| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Tâche d'export DOCX interrompue: {err}"),
        )
    })?
    .map_err(|err| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Impossible de générer le DOCX: {err}"),
        )
    });
    if export_result.is_err() {
        let _ = tokio::fs::remove_file(&temp_path).await;
    }
    export_result?;

    let bytes = tokio::fs::read(&temp_path).await.map_err(|err| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("DOCX généré mais impossible à relire: {err}"),
        )
    })?;
    let _ = tokio::fs::remove_file(&temp_path).await;

    Response::builder()
        .status(StatusCode::OK)
        .header(
            header::CONTENT_TYPE,
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        )
        .header(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{filename}\""),
        )
        .body(Body::from(bytes))
        .map_err(|err| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Impossible de préparer la réponse DOCX: {err}"),
            )
        })
}

async fn workdoc_save_state_handler(
    Json(payload): Json<WorkdocStateRequest>,
) -> Result<Json<WorkdocStateResponse>, (StatusCode, String)> {
    let db_path = workdoc_state_db_path();
    let document = payload.document;
    let saved = tokio::task::spawn_blocking(move || upsert_workdoc_state_at(&db_path, &document))
        .await
        .map_err(|err| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Sauvegarde du document interrompue: {err}"),
            )
        })?
        .map_err(workdoc_state_error_status)?;
    Ok(Json(saved))
}

async fn workdoc_get_state_handler(
    AxumPath(id): AxumPath<String>,
) -> Result<Json<WorkdocStateResponse>, (StatusCode, String)> {
    let db_path = workdoc_state_db_path();
    let document_id = id.clone();
    let loaded = tokio::task::spawn_blocking(move || load_workdoc_state_at(&db_path, &document_id))
        .await
        .map_err(|err| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Lecture du document interrompue: {err}"),
            )
        })?
        .map_err(workdoc_state_error_status)?;

    loaded.map(Json).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            format!("Document de travail introuvable: {id}"),
        )
    })
}

async fn workdoc_list_state_handler() -> Result<Json<WorkdocStateListResponse>, (StatusCode, String)>
{
    let db_path = workdoc_state_db_path();
    let documents = tokio::task::spawn_blocking(move || list_workdoc_states_at(&db_path))
        .await
        .map_err(|err| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Liste des documents interrompue: {err}"),
            )
        })?
        .map_err(workdoc_state_error_status)?;

    Ok(Json(WorkdocStateListResponse { documents }))
}

async fn workdoc_delete_state_handler(
    AxumPath(id): AxumPath<String>,
) -> Result<Json<WorkdocDeleteResponse>, (StatusCode, String)> {
    let db_path = workdoc_state_db_path();
    let document_id = id.clone();
    let deleted =
        tokio::task::spawn_blocking(move || delete_workdoc_state_at(&db_path, &document_id))
            .await
            .map_err(|err| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Suppression du document interrompue: {err}"),
                )
            })?
            .map_err(workdoc_state_error_status)?;

    Ok(Json(WorkdocDeleteResponse { id, deleted }))
}

fn workdoc_state_error_status(message: String) -> (StatusCode, String) {
    if message.contains("invalide")
        || message.contains("requis")
        || message.contains("trop volumineux")
        || message.contains("trop de questions")
    {
        (StatusCode::BAD_REQUEST, message)
    } else {
        (StatusCode::INTERNAL_SERVER_ERROR, message)
    }
}

fn validate_workdoc_export_payload(
    payload: &WorkdocExportRequest,
) -> Result<(), (StatusCode, String)> {
    if payload.markdown.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "Le livrable Markdown à exporter est vide.".to_string(),
        ));
    }
    let markdown_bytes = payload.markdown.len();
    if markdown_bytes > MAX_WORKDOC_EXPORT_MARKDOWN_BYTES {
        return Err((
            StatusCode::PAYLOAD_TOO_LARGE,
            format!(
                "Le livrable est trop volumineux pour l'export DOCX ({} octets, max {}).",
                markdown_bytes, MAX_WORKDOC_EXPORT_MARKDOWN_BYTES
            ),
        ));
    }
    Ok(())
}

fn workdoc_state_db_path() -> PathBuf {
    if let Some(path) = std::env::var_os("CODE_EXPLORER_WORKDOC_DB")
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
    {
        return path;
    }

    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(std::env::temp_dir)
        .join(".codeexplorer")
        .join("workdocs.sqlite3")
}

fn upsert_workdoc_state_at(
    db_path: &Path,
    document: &Value,
) -> Result<WorkdocStateResponse, String> {
    let metadata = validate_workdoc_state_document(document)?;
    let document_json = serde_json::to_string(document)
        .map_err(|err| format!("Document de travail invalide: {err}"))?;
    if document_json.len() > MAX_WORKDOC_STATE_BYTES {
        return Err(format!(
            "Document de travail trop volumineux ({} octets, max {}).",
            document_json.len(),
            MAX_WORKDOC_STATE_BYTES
        ));
    }

    let saved_at_unix_ms = unix_time_ms();
    let conn = open_workdoc_state_db_at(db_path)?;
    let response_id = metadata.id.clone();
    conn.execute(
        r#"
        INSERT INTO work_documents (
            id,
            filename,
            repo,
            repo_name,
            imported_at,
            updated_at,
            question_count,
            document_json
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
        ON CONFLICT(id) DO UPDATE SET
            filename = excluded.filename,
            repo = excluded.repo,
            repo_name = excluded.repo_name,
            imported_at = excluded.imported_at,
            updated_at = excluded.updated_at,
            question_count = excluded.question_count,
            document_json = excluded.document_json
        "#,
        params![
            metadata.id,
            metadata.filename,
            metadata.repo,
            metadata.repo_name,
            metadata.imported_at as i64,
            saved_at_unix_ms as i64,
            metadata.question_count as i64,
            document_json,
        ],
    )
    .map_err(|err| format!("Impossible de sauvegarder le document SQLite: {err}"))?;

    Ok(WorkdocStateResponse {
        id: response_id,
        saved_at_unix_ms,
        document: document.clone(),
    })
}

fn load_workdoc_state_at(db_path: &Path, id: &str) -> Result<Option<WorkdocStateResponse>, String> {
    let id = id.trim();
    if id.is_empty() {
        return Err("Identifiant de document requis.".to_string());
    }
    let conn = open_workdoc_state_db_at(db_path)?;
    let row = conn
        .query_row(
            "SELECT updated_at, document_json FROM work_documents WHERE id = ?1",
            params![id],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()
        .map_err(|err| format!("Impossible de charger le document SQLite: {err}"))?;

    row.map(|(saved_at, document_json)| {
        serde_json::from_str::<Value>(&document_json)
            .map_err(|err| format!("Document SQLite invalide: {err}"))
            .map(|document| WorkdocStateResponse {
                id: id.to_string(),
                saved_at_unix_ms: saved_at.max(0) as u64,
                document,
            })
    })
    .transpose()
}

fn list_workdoc_states_at(db_path: &Path) -> Result<Vec<WorkdocStateSummary>, String> {
    let conn = open_workdoc_state_db_at(db_path)?;
    let mut stmt = conn
        .prepare(
            r#"
            SELECT id, filename, repo, repo_name, imported_at, updated_at, question_count
            FROM work_documents
            ORDER BY updated_at DESC
            LIMIT ?1
            "#,
        )
        .map_err(|err| format!("Impossible de préparer la liste SQLite: {err}"))?;

    let rows = stmt
        .query_map(params![MAX_WORKDOC_STATE_LIST], |row| {
            Ok(WorkdocStateSummary {
                id: row.get(0)?,
                filename: row.get(1)?,
                repo: row.get(2)?,
                repo_name: row.get(3)?,
                imported_at: row.get::<_, i64>(4)?.max(0) as u64,
                saved_at_unix_ms: row.get::<_, i64>(5)?.max(0) as u64,
                question_count: row.get::<_, i64>(6)?.max(0) as usize,
            })
        })
        .map_err(|err| format!("Impossible de lire la liste SQLite: {err}"))?;

    let mut documents = Vec::new();
    for row in rows {
        documents.push(row.map_err(|err| format!("Document SQLite invalide: {err}"))?);
    }
    Ok(documents)
}

fn delete_workdoc_state_at(db_path: &Path, id: &str) -> Result<bool, String> {
    let id = id.trim();
    if id.is_empty() {
        return Err("Identifiant de document requis.".to_string());
    }
    let conn = open_workdoc_state_db_at(db_path)?;
    let changed = conn
        .execute("DELETE FROM work_documents WHERE id = ?1", params![id])
        .map_err(|err| format!("Impossible de supprimer le document SQLite: {err}"))?;
    Ok(changed > 0)
}

fn open_workdoc_state_db_at(db_path: &Path) -> Result<Connection, String> {
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|err| format!("Impossible de créer le dossier SQLite: {err}"))?;
    }
    let conn = Connection::open(db_path)
        .map_err(|err| format!("Impossible d'ouvrir la base SQLite: {err}"))?;
    conn.busy_timeout(Duration::from_secs(3))
        .map_err(|err| format!("Impossible de configurer SQLite: {err}"))?;
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS work_documents (
            id TEXT PRIMARY KEY,
            filename TEXT NOT NULL,
            repo TEXT,
            repo_name TEXT,
            imported_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            question_count INTEGER NOT NULL,
            document_json TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_work_documents_updated_at
            ON work_documents(updated_at DESC);
        "#,
    )
    .map_err(|err| format!("Impossible d'initialiser la base SQLite: {err}"))?;
    Ok(conn)
}

fn validate_workdoc_state_document(document: &Value) -> Result<WorkdocStateMetadata, String> {
    if !document.is_object() {
        return Err("Document de travail invalide: objet JSON requis.".to_string());
    }
    let id = read_workdoc_json_string(document, "id")
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .ok_or_else(|| "Document de travail invalide: id requis.".to_string())?
        .to_string();
    if id.chars().count() > 200 {
        return Err("Document de travail invalide: id trop long.".to_string());
    }

    let filename = read_workdoc_json_string(document, "filename")
        .map(str::trim)
        .filter(|filename| !filename.is_empty())
        .ok_or_else(|| "Document de travail invalide: filename requis.".to_string())?
        .to_string();
    if filename.chars().count() > 260 {
        return Err("Document de travail invalide: filename trop long.".to_string());
    }

    let question_count = document
        .get("questions")
        .and_then(Value::as_array)
        .ok_or_else(|| "Document de travail invalide: questions requis.".to_string())?
        .len();
    if question_count > MAX_WORKDOC_QUESTIONS {
        return Err(format!(
            "Document de travail invalide: trop de questions ({} max {}).",
            question_count, MAX_WORKDOC_QUESTIONS
        ));
    }

    Ok(WorkdocStateMetadata {
        id,
        filename,
        repo: read_workdoc_json_string(document, "repo").map(str::to_string),
        repo_name: read_workdoc_json_string(document, "repoName").map(str::to_string),
        imported_at: read_workdoc_json_u64(document, "importedAt").unwrap_or_else(unix_time_ms),
        question_count,
    })
}

fn read_workdoc_json_string<'a>(document: &'a Value, key: &str) -> Option<&'a str> {
    document.get(key).and_then(Value::as_str)
}

fn read_workdoc_json_u64(document: &Value, key: &str) -> Option<u64> {
    document.get(key).and_then(|value| {
        value
            .as_u64()
            .or_else(|| value.as_i64().map(|n| n.max(0) as u64))
    })
}

fn unix_time_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or_default()
}

async fn write_workdoc_temp_file(body: &Bytes) -> Result<PathBuf, String> {
    let dir = std::env::temp_dir().join("code-explorer-workdocs");
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|err| format!("Impossible de créer le dossier temporaire: {err}"))?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();
    let path = dir.join(format!("workdoc-{stamp}-{}.docx", uuid::Uuid::new_v4()));
    tokio::fs::write(&path, body)
        .await
        .map_err(|err| format!("Impossible d'écrire le document temporaire: {err}"))?;
    Ok(path)
}

async fn workdoc_temp_output_path(extension: &str) -> Result<PathBuf, String> {
    let dir = std::env::temp_dir().join("code-explorer-workdocs");
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|err| format!("Impossible de créer le dossier temporaire: {err}"))?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();
    Ok(dir.join(format!(
        "workdoc-export-{stamp}-{}.{}",
        uuid::Uuid::new_v4(),
        extension.trim_start_matches('.')
    )))
}

pub(crate) fn extract_workdoc_questions(markdown: &str) -> Vec<WorkdocQuestion> {
    let lines: Vec<String> = markdown
        .lines()
        .map(normalize_workdoc_line)
        .collect();

    let candidates = collect_workdoc_question_candidates(&lines);

    let mut seen = HashSet::new();
    let mut questions = Vec::new();
    for (candidate_index, (line_index, label, text)) in candidates.iter().enumerate() {
        let dedupe_key = normalize_question_key(text);
        if dedupe_key.is_empty() || !seen.insert(dedupe_key) {
            continue;
        }

        let next_line = candidates
            .get(candidate_index + 1)
            .map(|(next_index, _, _)| *next_index)
            .unwrap_or(lines.len());
        let context = collect_workdoc_context(&lines, *line_index, next_line);
        let order = questions.len() + 1;
        let display_label = if label == "Question" {
            format!("Q{order}")
        } else {
            label.clone()
        };
        questions.push(WorkdocQuestion {
            id: format!("q-{order:03}"),
            order,
            label: display_label,
            text: text.clone(),
            context,
        });
        if questions.len() >= MAX_WORKDOC_QUESTIONS {
            break;
        }
    }

    questions
}

fn collect_workdoc_question_groups(
    questions: &[WorkdocQuestion],
    colored_paragraphs: &[code_explorer_rag::docx::DocxColoredParagraph],
) -> Vec<WorkdocQuestionGroup> {
    if questions.is_empty() || colored_paragraphs.is_empty() {
        return Vec::new();
    }

    let mut groups: Vec<WorkdocQuestionGroup> = Vec::new();
    let mut group_index: HashMap<String, usize> = HashMap::new();
    for question in questions {
        let Some(color) = find_workdoc_question_color(question, colored_paragraphs) else {
            continue;
        };
        let group_id = format!(
            "color-{}-{}",
            color.family,
            slugify_workdoc_group_value(&color.value)
        );
        let index = *group_index.entry(group_id.clone()).or_insert_with(|| {
            let next_index = groups.len();
            groups.push(WorkdocQuestionGroup {
                id: group_id.clone(),
                label: color.label.clone(),
                color: WorkdocQuestionGroupColor {
                    family: color.family.clone(),
                    label: color.label.clone(),
                    value: color.value.clone(),
                },
                question_count: 0,
                question_ids: Vec::new(),
            });
            next_index
        });
        let group = &mut groups[index];
        group.question_count += 1;
        group.question_ids.push(question.id.clone());
    }

    groups
}

fn find_workdoc_question_color(
    question: &WorkdocQuestion,
    colored_paragraphs: &[code_explorer_rag::docx::DocxColoredParagraph],
) -> Option<code_explorer_rag::docx::DocxColorMark> {
    let question_key = normalize_question_key(&question.text);
    let question_probe = question_key
        .split_whitespace()
        .take(10)
        .collect::<Vec<_>>()
        .join(" ");
    if question_key.is_empty() || question_probe.chars().count() < 12 {
        return None;
    }

    colored_paragraphs.iter().find_map(|paragraph| {
        let color = paragraph.color.as_ref()?;
        let paragraph_key = normalize_question_key(&paragraph.text);
        if paragraph_key.is_empty() {
            return None;
        }
        let exact_or_contained =
            paragraph_key.contains(&question_key) || question_key.contains(&paragraph_key);
        let probe_match = paragraph_key.contains(&question_probe);
        (exact_or_contained || probe_match).then(|| color.clone())
    })
}

fn slugify_workdoc_group_value(value: &str) -> String {
    let slug = value
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect::<String>();
    if slug.is_empty() {
        "unknown".to_string()
    } else {
        slug
    }
}

fn normalize_workdoc_line(line: &str) -> String {
    if is_workdoc_markdown_image_line(line) {
        return String::new();
    }
    line.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn is_workdoc_markdown_image_line(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.starts_with("![") && trimmed.contains("](data:image/")
}

fn collect_workdoc_question_candidates(lines: &[String]) -> Vec<(usize, String, String)> {
    let mut candidates: Vec<(usize, String, String)> = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        if let Some((label, text)) = parse_workdoc_question_line(line) {
            candidates.push((index, label, text));
            continue;
        }
        if let Some(label) = parse_workdoc_question_label_line(line) {
            if let Some(text) = following_workdoc_question_text(lines, index + 1) {
                candidates.push((index, label, text));
            }
        }
    }
    candidates
}

fn parse_workdoc_question_line(line: &str) -> Option<(String, String)> {
    let line = line.trim();
    if line.is_empty() || line.eq_ignore_ascii_case("question posée") {
        return None;
    }
    let line = line.trim_start_matches('#').trim();
    if is_workdoc_url_line(line) {
        return None;
    }

    let q_code = regex::Regex::new(r"^(Q\d+(?:\.\d+)*)\s*[—–-]\s*(.{6,})$").unwrap();
    if let Some(caps) = q_code.captures(line) {
        let label = caps.get(1)?.as_str().trim().to_string();
        let text = caps.get(2)?.as_str().trim().to_string();
        return Some((label, text));
    }

    let numbered = regex::Regex::new(r"^(Question\s+\d+(?:\.\d+)?)\s*[:—–-]\s*(.{12,})$").unwrap();
    if let Some(caps) = numbered.captures(line) {
        let label = caps.get(1)?.as_str().trim().to_string();
        let text = caps.get(2)?.as_str().trim().to_string();
        return Some((label, text));
    }

    let explicit = regex::Regex::new(r#"^Question\s*:\s*[«"]?\s*(.{12,})$"#).unwrap();
    if let Some(caps) = explicit.captures(line) {
        let text = caps
            .get(1)?
            .as_str()
            .trim()
            .trim_matches(['»', '"'])
            .trim()
            .to_string();
        return Some(("Question".to_string(), text));
    }

    if is_plausible_workdoc_question_text(line) && is_french_question_text(line) {
        return Some(("Question".to_string(), normalize_question_text(line)));
    }

    None
}

fn parse_workdoc_question_label_line(line: &str) -> Option<String> {
    let line = line.trim().trim_start_matches('#').trim();
    if line.is_empty() {
        return None;
    }

    let q_code = regex::Regex::new(r"^Q\d+(?:\.\d+)*$").unwrap();
    if q_code.is_match(line) {
        return Some(line.to_string());
    }

    let numbered = regex::Regex::new(r"^Question\s+\d+(?:\.\d+)?$").unwrap();
    if numbered.is_match(line) {
        return Some(line.to_string());
    }

    None
}

fn following_workdoc_question_text(lines: &[String], start: usize) -> Option<String> {
    let mut inspected = 0usize;
    for line in lines.iter().skip(start) {
        let candidate = line.trim().trim_start_matches('#').trim();
        if candidate.is_empty() {
            continue;
        }
        if parse_workdoc_question_label_line(candidate).is_some() {
            return None;
        }
        if let Some((parsed_label, parsed_text)) = parse_workdoc_question_line(candidate) {
            return (parsed_label == "Question").then_some(parsed_text);
        }
        inspected += 1;
        if is_plausible_workdoc_question_text(candidate) {
            return Some(candidate.to_string());
        }
        if inspected >= 3 {
            return None;
        }
    }
    None
}

fn is_plausible_workdoc_question_text(text: &str) -> bool {
    let chars = text.chars().count();
    (6..=800).contains(&chars)
}

fn is_french_question_text(text: &str) -> bool {
    let normalized = text
        .trim()
        .trim_matches(['"', '«', '»'])
        .to_lowercase()
        .replace('’', "'");
    if normalized.contains('?') {
        return true;
    }
    let question_starters = [
        "a quoi ",
        "à quoi ",
        "d'ou ",
        "d'où ",
        "de quoi ",
        "depuis quoi ",
        "pourquoi ",
        "comment ",
        "combien ",
        "quand ",
        "que veut ",
        "que signifie ",
        "quel ",
        "quelle ",
        "quels ",
        "quelles ",
        "est-ce ",
        "est il ",
        "est-il ",
        "idem ici",
    ];
    question_starters
        .iter()
        .any(|starter| normalized.starts_with(starter))
}

fn normalize_question_text(text: &str) -> String {
    text.trim()
        .trim_matches(['"', '«', '»'])
        .trim()
        .trim_end_matches(':')
        .trim()
        .to_string()
}

fn is_workdoc_url_line(text: &str) -> bool {
    let trimmed = text.trim().to_ascii_lowercase();
    trimmed.starts_with("http://") || trimmed.starts_with("https://")
}

fn collect_workdoc_context(lines: &[String], start: usize, end: usize) -> String {
    let mut out = String::new();
    for line in &lines[start..end] {
        if line.trim().is_empty() {
            continue;
        }
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(line);
        if out.chars().count() >= WORKDOC_CONTEXT_CHARS {
            return out.chars().take(WORKDOC_CONTEXT_CHARS).collect();
        }
    }
    out
}

fn normalize_question_key(text: &str) -> String {
    text.chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .flat_map(|c| c.to_lowercase())
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn sanitize_workdoc_filename(filename: &str) -> String {
    let mut safe = filename
        .trim()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | ' ') {
                c
            } else {
                '-'
            }
        })
        .collect::<String>();
    while safe.contains("--") {
        safe = safe.replace("--", "-");
    }
    safe = safe.trim_matches([' ', '-', '.']).to_string();
    if safe.is_empty() {
        "document.docx".to_string()
    } else if safe.chars().count() > 160 {
        safe.chars().take(160).collect()
    } else {
        safe
    }
}

fn sanitize_workdoc_export_filename(filename: Option<&str>, extension: &str) -> String {
    let extension = extension.trim_start_matches('.').to_ascii_lowercase();
    let default_name = format!("code-explorer-document-travail.{extension}");
    let raw = filename.unwrap_or(&default_name).trim();
    let without_ext = raw
        .strip_suffix(&format!(".{extension}"))
        .or_else(|| raw.strip_suffix(&format!(".{}", extension.to_ascii_uppercase())))
        .unwrap_or(raw);
    let mut safe = without_ext
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_') {
                c
            } else if c.is_whitespace() {
                '-'
            } else if c.is_alphanumeric() {
                match c {
                    'é' | 'è' | 'ê' | 'ë' | 'É' | 'È' | 'Ê' | 'Ë' => 'e',
                    'à' | 'â' | 'ä' | 'À' | 'Â' | 'Ä' => 'a',
                    'ù' | 'û' | 'ü' | 'Ù' | 'Û' | 'Ü' => 'u',
                    'î' | 'ï' | 'Î' | 'Ï' => 'i',
                    'ô' | 'ö' | 'Ô' | 'Ö' => 'o',
                    'ç' | 'Ç' => 'c',
                    _ => '-',
                }
            } else {
                '-'
            }
        })
        .collect::<String>();
    while safe.contains("--") {
        safe = safe.replace("--", "-");
    }
    safe = safe.trim_matches([' ', '-', '.', '_']).to_string();
    if safe.is_empty() {
        default_name
    } else {
        format!(
            "{}.{}",
            safe.chars().take(120).collect::<String>(),
            extension
        )
    }
}

async fn chat_get_redirect() -> Redirect {
    Redirect::temporary("/")
}

fn resolve_chat_repo_entry<'a>(
    backend: &'a LocalBackend,
    requested_repo: &str,
    registry: &'a [RegistryEntry],
) -> Result<&'a RegistryEntry, (StatusCode, String)> {
    let requested_repo = requested_repo.trim();
    if requested_repo.is_empty() {
        return registry.first().ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                "No repository found. Run 'code-explorer analyze' first.".to_string(),
            )
        });
    }

    backend.resolve_repo(Some(requested_repo)).map_err(|_| {
        (
            StatusCode::NOT_FOUND,
            format!(
                "Repository '{requested_repo}' was not found in the Code Explorer registry. Refresh the project selector or run 'code-explorer analyze'."
            ),
        )
    })
}

async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("Failed to install ctrl+c handler");
    println!("\nShutting down...");
}

#[cfg(test)]
mod tests {
    use super::{
        collect_workdoc_question_groups, delete_workdoc_state_at, extract_workdoc_questions,
        is_loopback_host, is_loopback_origin, list_workdoc_states_at, load_workdoc_state_at,
        parse_workdoc_question_line, sanitize_workdoc_export_filename, sanitize_workdoc_filename,
        upsert_workdoc_state_at, validate_workdoc_export_payload, workdoc_export_docx_handler,
        workdoc_extract_handler, WorkdocExportRequest, WorkdocExtractQuery, WorkdocQuestion,
    };
    use axum::{
        body::{to_bytes, Bytes},
        extract::Query,
        http::{header, StatusCode},
        Json,
    };
    use base64::Engine as _;
    use serde_json::json;
    use std::fs::File;
    use std::io::Write;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};
    use zip::write::{FileOptions, SimpleFileOptions};

    fn write_impacts_questionnaire_fixture() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "code-explorer-workdoc-fixture-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("Questions - Impacts.docx");
        write_test_docx(&path);
        path
    }

    fn write_test_docx(path: &Path) {
        let file = File::create(path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let options: SimpleFileOptions =
            FileOptions::default().compression_method(zip::CompressionMethod::Deflated);

        zip.start_file("[Content_Types].xml", options).unwrap();
        zip.write_all(
            br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Default Extension="xml" ContentType="application/xml"/>
  <Default Extension="png" ContentType="image/png"/>
  <Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
</Types>"#,
        )
        .unwrap();

        zip.start_file("_rels/.rels", options).unwrap();
        zip.write_all(
            br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rIdDocument" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>"#,
        )
        .unwrap();

        zip.start_file("word/document.xml", options).unwrap();
        zip.write_all(generate_test_questionnaire_document_xml().as_bytes())
            .unwrap();

        zip.start_file("word/_rels/document.xml.rels", options)
            .unwrap();
        zip.write_all(generate_test_questionnaire_rels_xml().as_bytes())
            .unwrap();

        let tiny_png = base64::engine::general_purpose::STANDARD
            .decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==")
            .unwrap();
        for idx in 1..=28 {
            zip.start_file(format!("word/media/image{idx}.png"), options)
                .unwrap();
            zip.write_all(&tiny_png).unwrap();
        }

        zip.finish().unwrap();
    }

    fn generate_test_questionnaire_document_xml() -> String {
        let mut body = String::new();
        body.push_str(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#);
        body.push_str(
            r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><w:body>"#,
        );
        body.push_str(
            r#"<w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t>Impacts</w:t></w:r></w:p>"#,
        );
        for idx in 1..=28 {
            body.push_str(&format!(
                r#"<w:p><w:r><w:drawing><a:blip r:embed="rId{idx}"/></w:drawing></w:r></w:p>"#
            ));
        }
        for idx in 1..=27 {
            body.push_str(&format!(
                "<w:p><w:r><w:t>Q{idx} - Quelle est la portee fonctionnelle de l'impact numero {idx} ?</w:t></w:r></w:p>"
            ));
        }
        body.push_str(r#"<w:sectPr/></w:body></w:document>"#);
        body
    }

    fn generate_test_questionnaire_rels_xml() -> String {
        let mut rels = String::from(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">"#,
        );
        for idx in 1..=28 {
            rels.push_str(&format!(
                r#"<Relationship Id="rId{idx}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/image{idx}.png"/>"#
            ));
        }
        rels.push_str("</Relationships>");
        rels
    }

    #[test]
    fn loopback_host_detection_accepts_local_hosts() {
        assert!(is_loopback_host("127.0.0.1"));
        assert!(is_loopback_host("localhost"));
        assert!(is_loopback_host("::1"));
        assert!(is_loopback_host("[::1]"));
    }

    #[test]
    fn loopback_host_detection_rejects_network_binds() {
        assert!(!is_loopback_host("0.0.0.0"));
        assert!(!is_loopback_host("::"));
        assert!(!is_loopback_host("192.168.1.10"));
    }

    #[test]
    fn loopback_origin_detection_accepts_any_local_port() {
        assert!(is_loopback_origin("http://localhost:5175"));
        assert!(is_loopback_origin("http://127.0.0.1:5177"));
        assert!(is_loopback_origin("https://[::1]:1420"));
    }

    #[test]
    fn loopback_origin_detection_rejects_remote_and_non_http_origins() {
        assert!(!is_loopback_origin("http://192.168.1.10:5175"));
        assert!(!is_loopback_origin("https://example.com"));
        assert!(!is_loopback_origin("file://localhost/tmp"));
        assert!(!is_loopback_origin("null"));
        assert!(!is_loopback_origin("http://localhost:5175/sneaky"));
        assert!(!is_loopback_origin("http://user@localhost:5175"));
    }

    #[test]
    fn workdoc_question_parser_detects_common_shapes() {
        assert_eq!(
            parse_workdoc_question_line("Q1.1 — Paramétrage Création Groupe d'Aide"),
            Some((
                "Q1.1".to_string(),
                "Paramétrage Création Groupe d'Aide".to_string()
            ))
        );
        assert_eq!(
            parse_workdoc_question_line(
                "Question 12 : D'après le métier, quelles règles sont appliquées ?"
            ),
            Some((
                "Question 12".to_string(),
                "D'après le métier, quelles règles sont appliquées ?".to_string()
            ))
        );
        assert_eq!(
            parse_workdoc_question_line("Question : « À quoi correspondent ces paramétrages ? »"),
            Some((
                "Question".to_string(),
                "À quoi correspondent ces paramétrages ?".to_string()
            ))
        );
        assert_eq!(
            parse_workdoc_question_line("A quoi correspondent ces paramétrages :"),
            Some((
                "Question".to_string(),
                "A quoi correspondent ces paramétrages".to_string()
            ))
        );
        assert_eq!(
            parse_workdoc_question_line(
                "https://alisev2-qualif.asmeg.org/Dossiers/DetailsDossier?NumDossier=6210512859"
            ),
            None
        );
    }

    #[test]
    fn workdoc_extraction_deduplicates_questions_and_keeps_context() {
        let markdown = r#"
# Atelier
Q1.1 — Paramétrage Création Groupe d'Aide
Contexte métier important.
Question : À quoi correspondent ces paramétrages ?
Réponse déjà présente dans le document source.
Question : À quoi correspondent ces paramétrages ?
Duplicat à ignorer.
Question 2 : Comment le paiement est-il déclenché ?
Contexte technique.
"#;

        let questions = extract_workdoc_questions(markdown);

        assert_eq!(questions.len(), 3);
        assert_eq!(questions[0].id, "q-001");
        assert_eq!(questions[0].label, "Q1.1");
        assert!(questions[0].context.contains("Contexte métier important"));
        assert_eq!(questions[1].text, "À quoi correspondent ces paramétrages ?");
        assert_eq!(questions[1].label, "Q2");
        assert_eq!(questions[2].label, "Question 2");
    }

    #[test]
    fn workdoc_extraction_accepts_word_split_question_labels() {
        let markdown = r#"
# Atelier Sample
Q1.1
Paramétrage Création Groupe d'Aide
Contexte métier important.

Question 2
Comment le paiement est-il déclenché ?
Contexte technique.
"#;

        let questions = extract_workdoc_questions(markdown);

        assert_eq!(questions.len(), 2);
        assert_eq!(questions[0].label, "Q1.1");
        assert_eq!(questions[0].text, "Paramétrage Création Groupe d'Aide");
        assert!(questions[0].context.contains("Contexte métier important"));
        assert_eq!(questions[1].label, "Question 2");
        assert_eq!(questions[1].text, "Comment le paiement est-il déclenché ?");
    }

    #[test]
    fn workdoc_question_groups_follow_word_colors() {
        let questions = vec![
            WorkdocQuestion {
                id: "q-001".to_string(),
                order: 1,
                label: "Q1".to_string(),
                text: "Ancienne question à oublier ?".to_string(),
                context: String::new(),
            },
            WorkdocQuestion {
                id: "q-002".to_string(),
                order: 2,
                label: "Q2".to_string(),
                text: "Nouvelle question à traiter ?".to_string(),
                context: String::new(),
            },
        ];
        let paragraphs = vec![
            code_explorer_rag::docx::DocxColoredParagraph {
                text: "Ancienne question à oublier ?".to_string(),
                color: Some(code_explorer_rag::docx::DocxColorMark {
                    family: "green".to_string(),
                    label: "Vert".to_string(),
                    value: "00B050".to_string(),
                }),
            },
            code_explorer_rag::docx::DocxColoredParagraph {
                text: "Nouvelle question à traiter ?".to_string(),
                color: Some(code_explorer_rag::docx::DocxColorMark {
                    family: "blue".to_string(),
                    label: "Bleu".to_string(),
                    value: "0070C0".to_string(),
                }),
            },
        ];

        let groups = collect_workdoc_question_groups(&questions, &paragraphs);

        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].label, "Vert");
        assert_eq!(groups[0].question_ids, vec!["q-001"]);
        assert_eq!(groups[1].label, "Bleu");
        assert_eq!(groups[1].question_ids, vec!["q-002"]);
    }

    #[test]
    fn workdoc_extraction_matches_generated_impacts_questionnaire_fixture() {
        let fixture = write_impacts_questionnaire_fixture();

        let markdown = code_explorer_rag::docx::docx_to_markdown(&fixture)
            .expect("questionnaire fixture should convert to markdown");
        let questions = extract_workdoc_questions(&markdown);

        assert_eq!(questions.len(), 27);
        assert_eq!(questions[0].id, "q-001");
        assert_eq!(questions[0].label, "Q1");
        assert!(
            questions
                .iter()
                .all(|question| !question.text.starts_with("http")),
            "URL-only lines must not become work-document questions"
        );

        std::fs::remove_dir_all(fixture.parent().unwrap()).unwrap();
    }

    #[tokio::test]
    async fn workdoc_extract_handler_returns_react_contract_for_generated_docx() {
        let fixture = write_impacts_questionnaire_fixture();
        let bytes = std::fs::read(&fixture).expect("fixture should be readable");

        let Json(payload) = workdoc_extract_handler(
            Query(WorkdocExtractQuery {
                file_name: Some("Questions - Impacts.docx".to_string()),
            }),
            Bytes::from(bytes),
        )
        .await
        .expect("generated questionnaire fixture should be accepted");

        assert_eq!(payload.document.filename, "Questions - Impacts.docx");
        assert_eq!(payload.questions.len(), 27);
        assert!(payload.document.markdown_chars > payload.questions.len());
        assert!(payload.source_markdown.contains("Impacts"));
        assert_eq!(
            payload
                .source_markdown
                .matches("](data:image/png;base64,")
                .count(),
            28
        );
        assert!(
            payload
                .questions
                .iter()
                .all(|question| !question.context.contains("data:image/")),
            "base64 images should not leak into question prompts"
        );

        let json_payload = serde_json::to_value(&payload).expect("payload should serialize");
        assert!(json_payload.get("sourceMarkdown").is_some());
        assert!(json_payload["document"].get("markdownChars").is_some());

        std::fs::remove_dir_all(fixture.parent().unwrap()).unwrap();
    }

    #[tokio::test]
    async fn workdoc_export_docx_handler_returns_downloadable_docx() {
        let payload = WorkdocExportRequest {
            filename: Some("Réponses Sample / finale.docx".to_string()),
            title: Some("Réponses Code Explorer".to_string()),
            markdown: [
                "# Réponses Code Explorer",
                "",
                "| Métadonnée | Valeur |",
                "|---|---|",
                "| Projet | sample-app |",
                "",
                "## Questions et réponses détaillées",
                "",
                "### Chapitre 1 - Q1",
                "",
                "Réponse vérifiée.",
            ]
            .join("\n"),
        };

        let response = workdoc_export_docx_handler(Json(payload))
            .await
            .expect("valid markdown should export as DOCX");

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE).unwrap(),
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
        );
        assert_eq!(
            response.headers().get(header::CONTENT_DISPOSITION).unwrap(),
            "attachment; filename=\"Reponses-Sample-finale.docx\""
        );
        let bytes = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("DOCX response body should be readable");
        assert!(
            bytes.starts_with(b"PK"),
            "DOCX response should be a zip package"
        );
        assert!(bytes.len() > 1_000);
    }

    #[test]
    fn workdoc_filename_sanitizer_keeps_docx_extension() {
        assert_eq!(
            sanitize_workdoc_filename("Questions:/Sample?.docx"),
            "Questions-Sample-.docx"
        );
        assert_eq!(sanitize_workdoc_filename(" ... "), "document.docx");
    }

    #[test]
    fn workdoc_export_payload_validation_rejects_empty_markdown() {
        let payload = WorkdocExportRequest {
            filename: None,
            title: None,
            markdown: "   ".to_string(),
        };
        assert!(validate_workdoc_export_payload(&payload)
            .unwrap_err()
            .1
            .contains("vide"));
    }

    #[test]
    fn workdoc_state_sqlite_round_trips_snapshot() {
        let db_path = std::env::temp_dir().join(format!(
            "code-explorer-workdoc-state-test-{}.sqlite3",
            uuid::Uuid::new_v4()
        ));
        let document = json!({
            "id": "doc-1",
            "filename": "Questions Sample.docx",
            "importedAt": 1774500000000_u64,
            "repo": "repo_alise",
            "repoName": "sample-app",
            "sourceBytes": 1200,
            "markdownChars": 4200,
            "sourceMarkdown": "# Source",
            "questions": [
                {
                    "id": "q-1",
                    "order": 1,
                    "label": "Q1",
                    "text": "A quoi sert le workflow ?",
                    "context": "Chapitre 1",
                    "status": "answered",
                    "answer": "Réponse.\n\n## Sources\n- src/workflow.cs"
                }
            ]
        });

        let saved = upsert_workdoc_state_at(&db_path, &document)
            .expect("snapshot should be saved in SQLite");
        assert_eq!(saved.id, "doc-1");

        let loaded = load_workdoc_state_at(&db_path, "doc-1")
            .expect("snapshot should load")
            .expect("snapshot should exist");
        assert_eq!(loaded.document["filename"], "Questions Sample.docx");
        assert_eq!(loaded.document["questions"][0]["status"], "answered");

        let summaries = list_workdoc_states_at(&db_path).expect("summaries should load");
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].question_count, 1);
        assert_eq!(summaries[0].repo_name.as_deref(), Some("sample-app"));

        assert!(delete_workdoc_state_at(&db_path, "doc-1").expect("delete should succeed"));
        assert!(load_workdoc_state_at(&db_path, "doc-1")
            .expect("load after delete should succeed")
            .is_none());
        let _ = std::fs::remove_file(db_path);
    }

    #[test]
    fn workdoc_export_filename_sanitizer_is_ascii_safe() {
        assert_eq!(
            sanitize_workdoc_export_filename(Some("Réponses Sample / finale.docx"), "docx"),
            "Reponses-Sample-finale.docx"
        );
        assert_eq!(
            sanitize_workdoc_export_filename(Some("../secret"), "docx"),
            "secret.docx"
        );
        assert_eq!(
            sanitize_workdoc_export_filename(Some("   "), "docx"),
            "code-explorer-document-travail.docx"
        );
    }
}

//! HTTP transport for MCP via axum.
//!
//! Provides JSON-RPC endpoint (POST /mcp) and REST API endpoints for
//! direct HTTP integration without MCP protocol overhead.

use std::collections::{BTreeMap, HashSet};
use std::net::Ipv4Addr;
use std::path::{Component, Path as FsPath, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axum::{
    body::{Body, Bytes},
    extract::{DefaultBodyLimit, Path, Query, Request, State},
    http::{header, HeaderValue, Method, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::{process::Command, sync::Mutex, time::timeout};
use tower_http::cors::CorsLayer;

use code_explorer_core::llm::sanitize_llm_error_body;
use code_explorer_core::secret_store::{decode_secret_from_storage, secret_payload_needs_migration};
use code_explorer_core::storage::repo_manager::registry_entry_id;
use code_explorer_core::{
    config::languages::SupportedLanguage,
    graph::types::{GraphNode, GraphRelationship, NodeLabel, RelationshipType},
};

use crate::backend::local::LocalBackend;
use crate::jsonrpc::{JsonRpcRequest, JsonRpcResponse};
use crate::server::handle_request;

/// Shared state for the HTTP handler.
pub type SharedBackend = Arc<Mutex<LocalBackend>>;

const NATIVE_PRINT_PDF_JS: &str = include_str!("native-print-pdf.js");

fn http_auth_token() -> Option<Arc<String>> {
    std::env::var("CODE_EXPLORER_HTTP_TOKEN")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .map(Arc::new)
}

fn expose_repo_paths() -> bool {
    matches!(
        std::env::var("CODE_EXPLORER_EXPOSE_REPO_PATHS").ok().as_deref(),
        Some("1" | "true" | "TRUE" | "yes" | "YES")
    )
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

/// Create an axum Router with MCP and REST API endpoints.
pub fn mcp_http_router() -> Router<SharedBackend> {
    let protected = Router::new()
        .route("/mcp", post(mcp_handler))
        .route("/api/repos", get(repos_handler))
        .route(
            "/api/llm-config",
            get(llm_config_handler).post(save_llm_config_handler),
        )
        .route("/api/llm-config/models", post(llm_config_models_handler))
        .route("/api/llm-config/tailnet-hosts", get(tailnet_hosts_handler))
        .route("/api/llm-config/test", post(test_llm_config_handler))
        .route("/api/diagnostics", get(diagnostics_handler))
        .route(
            "/api/export/pdf",
            post(export_pdf_handler).layer(DefaultBodyLimit::max(MAX_PDF_HTML_BYTES + 1024)),
        )
        .route("/api/repos/:name/search", get(search_handler))
        .route("/api/repos/:name/symbols", get(symbols_handler))
        .route("/api/repos/:name/files", get(files_handler))
        .route("/api/repos/:name/source", get(source_handler))
        .route("/api/repos/:name/graph", get(graph_handler))
        .route(
            "/api/repos/:name/graph/neighborhood",
            get(graph_neighborhood_handler),
        )
        .route("/api/repos/:name/stats", get(stats_handler))
        .route("/api/repos/:name/hotspots", get(hotspots_handler));

    let protected = if let Some(token) = http_auth_token() {
        protected.route_layer(middleware::from_fn_with_state(token, auth_middleware))
    } else {
        protected
    };

    Router::new()
        .route("/health", get(health_handler))
        .merge(protected)
        .layer(
            CorsLayer::new()
                .allow_origin([
                    "http://localhost".parse::<HeaderValue>().unwrap(),
                    "http://localhost:3000".parse::<HeaderValue>().unwrap(),
                    "http://localhost:3010".parse::<HeaderValue>().unwrap(),
                    "http://127.0.0.1".parse::<HeaderValue>().unwrap(),
                    "http://127.0.0.1:3000".parse::<HeaderValue>().unwrap(),
                    "http://127.0.0.1:3010".parse::<HeaderValue>().unwrap(),
                    "http://localhost:1420".parse::<HeaderValue>().unwrap(),
                    "http://localhost:5174".parse::<HeaderValue>().unwrap(),
                    "http://localhost:5176".parse::<HeaderValue>().unwrap(),
                    "http://127.0.0.1:5174".parse::<HeaderValue>().unwrap(),
                    "http://127.0.0.1:5176".parse::<HeaderValue>().unwrap(),
                ])
                .allow_methods([Method::GET, Method::POST])
                .allow_headers(tower_http::cors::Any),
        )
}

/// Handle a POST /mcp request: parse JSON-RPC, dispatch, respond.
///
/// Parses the body as raw bytes (instead of `Json<JsonRpcRequest>`) so that
/// malformed JSON or schema mismatches return a JSON-RPC 2.0 -32700 (parse
/// error) / -32600 (invalid request) response with HTTP 200, not axum's
/// default 422 plain-text response. MCP clients only know how to handle
/// JSON-RPC envelopes — a 422 leaves them in an undefined state.
async fn mcp_handler(State(backend): State<SharedBackend>, body: Bytes) -> impl IntoResponse {
    // Try to extract the request id up front so any error response can echo it
    // back per JSON-RPC spec; fall back to null if parsing fails entirely.
    let raw_json: Result<Value, _> = serde_json::from_slice(&body);
    let id = raw_json
        .as_ref()
        .ok()
        .and_then(|v| v.get("id").cloned())
        .unwrap_or(Value::Null);

    let request: JsonRpcRequest = match raw_json {
        Ok(v) => match serde_json::from_value::<JsonRpcRequest>(v) {
            Ok(r) => r,
            Err(e) => {
                return (
                    StatusCode::OK,
                    Json(JsonRpcResponse::error(
                        id,
                        -32600, // Invalid Request
                        format!("Invalid JSON-RPC request: {e}"),
                        None,
                    )),
                );
            }
        },
        Err(e) => {
            return (
                StatusCode::OK,
                Json(JsonRpcResponse::error(
                    id,
                    -32700, // Parse error
                    format!("Parse error: {e}"),
                    None,
                )),
            );
        }
    };

    let mut backend_guard = backend.lock().await;
    let response = handle_request(&request, &mut backend_guard).await;
    (StatusCode::OK, Json(response))
}

/// GET /health — Liveness check
async fn health_handler() -> impl IntoResponse {
    Json(json!({
        "status": "ok",
        "service": "code-explorer",
        "version": env!("CARGO_PKG_VERSION"),
    }))
}

/// GET /api/llm-config — LLM model metadata for the local UI.
///
/// Deliberately omits secrets (`api_key`) and provider endpoint details.
async fn llm_config_handler() -> impl IntoResponse {
    Json(llm_config_payload())
}

/// POST /api/llm-config — update the local LLM provider config.
///
/// The payload intentionally treats `apiKey` as write-only. If the UI omits
/// it, we preserve the existing stored key only when the provider stays the
/// same; otherwise the new provider relies on environment variables or a
/// newly-entered key. Responses keep returning the same secret-free payload as
/// GET /api/llm-config.
async fn save_llm_config_handler(
    Json(update): Json<LlmConfigUpdate>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let config = normalize_llm_config_update(update)
        .map_err(|message| api_error(StatusCode::BAD_REQUEST, message))?;
    crate::llm_config::save_llm_config(&config)
        .map_err(|err| api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;
    Ok(Json(llm_config_payload()))
}

/// POST /api/llm-config/models — list models exposed by an OpenAI-compatible endpoint.
async fn llm_config_models_handler(
    Json(request): Json<LlmModelsRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let base_url = request.base_url.trim().trim_end_matches('/').to_string();
    if base_url.is_empty() {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "L'URL de base du fournisseur LLM est obligatoire.",
        ));
    }
    if !is_allowed_llm_base_url(&base_url) {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "L'URL LLM doit être en HTTPS, ou en HTTP local/privé/Tailscale.",
        ));
    }
    let api_key = request
        .api_key
        .map(|value| value.trim().to_string())
        .unwrap_or_default();
    if base_url.to_ascii_lowercase().starts_with("http://")
        && !is_loopback_llm_base_url(&base_url)
        && !api_key.is_empty()
    {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "Refus d'envoyer une clé API vers un endpoint HTTP non local.",
        ));
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .build()
        .map_err(|err| api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;
    let url = format!("{}/models", base_url);
    let mut http_request = client.get(&url);
    if !api_key.is_empty() {
        http_request = http_request.header("Authorization", format!("Bearer {api_key}"));
    }

    let response = http_request.send().await.map_err(|err| {
        api_error(
            StatusCode::BAD_GATEWAY,
            format!("Impossible de lire les modèles LLM: {err}"),
        )
    })?;
    let status = response.status();
    let raw = response.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(api_error(
            StatusCode::BAD_GATEWAY,
            format!(
                "Endpoint modèles LLM en erreur ({}): {}",
                status,
                code_explorer_core::llm::sanitize_llm_error_body(&raw, &[&api_key], 500)
            ),
        ));
    }

    let parsed: Value = serde_json::from_str(&raw).map_err(|err| {
        api_error(
            StatusCode::BAD_GATEWAY,
            format!("Réponse modèles LLM invalide: {err}"),
        )
    })?;
    let mut models = parse_openai_model_list(&parsed);
    models.sort();
    models.dedup();

    Ok(Json(json!({
        "provider": request.provider.trim(),
        "baseUrl": base_url,
        "models": models,
    })))
}

/// GET /api/llm-config/tailnet-hosts — online state for Tailscale machines.
async fn tailnet_hosts_handler() -> impl IntoResponse {
    Json(tailnet_hosts_payload().await)
}

async fn tailnet_hosts_payload() -> Value {
    let output = match timeout(
        std::time::Duration::from_secs(3),
        Command::new("tailscale")
            .args(["status", "--json"])
            .output(),
    )
    .await
    {
        Ok(Ok(output)) => output,
        Ok(Err(err)) => {
            return json!({
                "available": false,
                "hosts": [],
                "error": format!("Commande tailscale indisponible: {err}"),
            });
        }
        Err(_) => {
            return json!({
                "available": false,
                "hosts": [],
                "error": "Commande tailscale trop lente.",
            });
        }
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return json!({
            "available": false,
            "hosts": [],
            "error": format!("tailscale status a échoué: {}", stderr.trim()),
        });
    }

    let raw = String::from_utf8_lossy(&output.stdout);
    match parse_tailscale_status_hosts(&raw) {
        Ok(hosts) => json!({
            "available": true,
            "hosts": hosts,
            "error": Value::Null,
        }),
        Err(err) => json!({
            "available": false,
            "hosts": [],
            "error": format!("Réponse tailscale status invalide: {err}"),
        }),
    }
}

fn parse_tailscale_status_hosts(raw: &str) -> Result<Vec<Value>, serde_json::Error> {
    let status: TailscaleStatusPayload = serde_json::from_str(raw)?;
    let mut hosts = Vec::new();

    if let Some(self_node) = status.self_node {
        if let Some(host) = tailscale_host_value(&self_node) {
            hosts.push(host);
        }
    }

    for peer in status.peers.values() {
        if let Some(host) = tailscale_host_value(peer) {
            hosts.push(host);
        }
    }

    hosts.sort_by(|a, b| {
        a["hostName"]
            .as_str()
            .unwrap_or_default()
            .cmp(b["hostName"].as_str().unwrap_or_default())
    });
    Ok(hosts)
}

fn tailscale_host_value(peer: &TailscalePeer) -> Option<Value> {
    let host_name = peer.host_name.as_deref()?.trim();
    if host_name.is_empty() {
        return None;
    }
    let ipv4 = peer
        .tailscale_ips
        .iter()
        .find(|ip| ip.contains('.'))
        .cloned();

    Some(json!({
        "hostName": host_name,
        "dnsName": peer.dns_name.as_deref().unwrap_or_default().trim_end_matches('.'),
        "os": peer.os.as_deref().unwrap_or_default(),
        "tailscaleIp": ipv4,
        "online": peer.online,
        "active": peer.active,
    }))
}

/// POST /api/llm-config/test — validate a provider without saving it.
async fn test_llm_config_handler(
    Json(update): Json<LlmConfigUpdate>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let config = normalize_llm_config_update(update)
        .map_err(|message| api_error(StatusCode::BAD_REQUEST, message))?;
    let provider = crate::llm_config::display_provider(&config);
    if provider.eq_ignore_ascii_case("chatgpt") {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "Le test direct cible les endpoints OpenAI-compatible. Pour ChatGPT OAuth, utilise le diagnostic/login existant.",
        ));
    }

    let max_tokens = config.max_tokens.unwrap_or(24).min(64);
    let started = SystemTime::now();
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(45),
        test_openai_compatible_config(&config, max_tokens),
    )
    .await
    .map_err(|_| {
        api_error(
            StatusCode::GATEWAY_TIMEOUT,
            "Test LLM expiré après 45 secondes.",
        )
    })?;
    let response = result.map_err(|err| {
        api_error(
            StatusCode::BAD_GATEWAY,
            sanitize_llm_error_body(&err, &[&config.api_key], 500),
        )
    })?;
    let elapsed_ms = started
        .elapsed()
        .map(|duration| duration.as_millis())
        .unwrap_or_default();

    Ok(Json(json!({
        "ok": true,
        "provider": provider,
        "model": config.model,
        "elapsedMs": elapsed_ms,
        "responsePreview": response.trim().chars().take(120).collect::<String>(),
    })))
}

async fn test_openai_compatible_config(
    config: &crate::llm_config::LlmConfig,
    max_tokens: u32,
) -> Result<String, String> {
    let url = format!("{}/chat/completions", config.base_url.trim_end_matches('/'));
    let mut body = json!({
        "model": config.model,
        "messages": [
            {
                "role": "system",
                "content": "Tu es un test de connectivite. Reponds seulement avec OK."
            },
            {
                "role": "user",
                "content": "Reponds uniquement: OK"
            }
        ],
        "max_tokens": max_tokens,
        "temperature": 0.0,
        "stream": false,
    });

    if let Some(reasoning) = config.reasoning_effort.as_deref() {
        let reasoning = reasoning.trim().to_ascii_lowercase();
        if !reasoning.is_empty() && reasoning != "none" {
            body["reasoning_effort"] = Value::String(reasoning);
        }
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(45))
        .build()
        .map_err(|err| format!("Client HTTP LLM invalide: {err}"))?;
    let mut request = client.post(&url).json(&body);
    if !config.api_key.trim().is_empty() {
        request = request.header("Authorization", format!("Bearer {}", config.api_key));
    }

    let response = request
        .send()
        .await
        .map_err(|err| format!("LLM API request failed: {err}"))?;
    let status = response.status();
    if !status.is_success() {
        let error_text = response.text().await.unwrap_or_default();
        return Err(format!(
            "LLM API error ({}): {}",
            status,
            sanitize_llm_error_body(&error_text, &[&config.api_key], 300)
        ));
    }

    let payload = response
        .json::<Value>()
        .await
        .map_err(|err| format!("Réponse LLM JSON invalide: {err}"))?;
    llm_test_response_preview(&payload)
}

fn llm_test_response_preview(payload: &Value) -> Result<String, String> {
    let message = payload
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("message"))
        .ok_or_else(|| "Réponse LLM sans choices[0].message.".to_string())?;
    if let Some(content) = message
        .get("content")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|content| !content.is_empty())
    {
        return Ok(content.chars().take(120).collect());
    }
    let has_reasoning = ["reasoning", "reasoning_content"].iter().any(|field| {
        message
            .get(*field)
            .and_then(Value::as_str)
            .map(str::trim)
            .is_some_and(|content| !content.is_empty())
    });
    if has_reasoning {
        return Ok("Réponse reçue (raisonnement interne sans contenu final visible).".to_string());
    }
    Ok("Réponse reçue sans contenu textuel visible.".to_string())
}

fn llm_config_payload() -> Value {
    match crate::llm_config::load_llm_config() {
        Some(config) => json!({
            "configured": true,
            "provider": crate::llm_config::display_provider(&config),
            "model": config.model,
            "reasoningEffort": config.reasoning_effort,
            "maxTokens": config.max_tokens,
            "bigContextModel": config.big_context_model,
        }),
        None => json!({ "configured": false }),
    }
}

#[derive(Debug, Deserialize)]
struct LlmConfigUpdate {
    #[serde(default)]
    provider: String,
    #[serde(default, alias = "apiKey")]
    api_key: Option<String>,
    #[serde(default, alias = "baseUrl")]
    base_url: String,
    #[serde(default)]
    model: String,
    #[serde(default, alias = "reasoningEffort")]
    reasoning_effort: Option<String>,
    #[serde(default, alias = "maxTokens")]
    max_tokens: Option<u32>,
    #[serde(default, alias = "bigContextModel")]
    big_context_model: Option<String>,
    #[serde(default, alias = "bigContextThresholdBytes")]
    big_context_threshold_bytes: Option<usize>,
    #[serde(default, alias = "bigContextMaxTokens")]
    big_context_max_tokens: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct LlmModelsRequest {
    #[serde(default)]
    provider: String,
    #[serde(default, alias = "apiKey")]
    api_key: Option<String>,
    #[serde(default, alias = "baseUrl")]
    base_url: String,
}

#[derive(Debug, Deserialize)]
struct TailscaleStatusPayload {
    #[serde(rename = "Self")]
    self_node: Option<TailscalePeer>,
    #[serde(rename = "Peer", default)]
    peers: BTreeMap<String, TailscalePeer>,
}

#[derive(Debug, Deserialize)]
struct TailscalePeer {
    #[serde(rename = "HostName")]
    host_name: Option<String>,
    #[serde(rename = "DNSName")]
    dns_name: Option<String>,
    #[serde(rename = "OS")]
    os: Option<String>,
    #[serde(rename = "TailscaleIPs", default)]
    tailscale_ips: Vec<String>,
    #[serde(rename = "Online", default)]
    online: bool,
    #[serde(rename = "Active", default)]
    active: bool,
}

fn parse_openai_model_list(value: &Value) -> Vec<String> {
    value
        .get("data")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.get("id").and_then(Value::as_str))
                .map(str::trim)
                .filter(|id| !id.is_empty())
                .filter(|id| !id.to_ascii_lowercase().contains("embed"))
                .map(ToOwned::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn normalize_llm_config_update(
    update: LlmConfigUpdate,
) -> Result<crate::llm_config::LlmConfig, String> {
    let provider = update.provider.trim().to_ascii_lowercase();
    let base_url = update.base_url.trim().trim_end_matches('/').to_string();
    let model = update.model.trim().to_string();

    if provider.is_empty() {
        return Err("Le fournisseur LLM est obligatoire.".to_string());
    }
    if provider.len() > 64
        || !provider
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    {
        return Err("Le fournisseur LLM contient des caractères non supportés.".to_string());
    }
    if model.is_empty() {
        return Err("Le modèle LLM est obligatoire.".to_string());
    }
    if model.len() > 160 {
        return Err("Le nom du modèle LLM est trop long.".to_string());
    }
    if base_url.is_empty() {
        return Err("L'URL de base du fournisseur LLM est obligatoire.".to_string());
    }
    if !is_allowed_llm_base_url(&base_url) {
        return Err("L'URL LLM doit être en HTTPS, ou en HTTP local/privé/Tailscale.".to_string());
    }
    if let Some(max_tokens) = update.max_tokens {
        if max_tokens == 0 || max_tokens > 262_144 {
            return Err("Le budget max_tokens doit être compris entre 1 et 262144.".to_string());
        }
    }

    let reasoning_effort = update
        .reasoning_effort
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty());
    if let Some(effort) = reasoning_effort.as_deref() {
        if !matches!(
            effort,
            "none" | "minimal" | "low" | "medium" | "high" | "xhigh"
        ) {
            return Err(
                "Le niveau de raisonnement doit être none, minimal, low, medium, high ou xhigh."
                    .to_string(),
            );
        }
    }

    let existing = crate::llm_config::load_llm_config_raw();
    let same_provider = existing
        .as_ref()
        .map(crate::llm_config::display_provider)
        .map(|value| value.eq_ignore_ascii_case(&provider))
        .unwrap_or(false);
    let api_key = match update.api_key {
        Some(value) => value.trim().to_string(),
        None if same_provider => existing
            .as_ref()
            .map(|config| config.api_key.clone())
            .unwrap_or_default(),
        None => String::new(),
    };
    if base_url.to_ascii_lowercase().starts_with("http://")
        && !is_loopback_llm_base_url(&base_url)
        && !api_key.is_empty()
    {
        return Err("Refus d'enregistrer une clé API vers un endpoint HTTP non local. Utilise HTTPS ou laisse la clé vide pour Ollama/Tailscale.".to_string());
    }

    Ok(crate::llm_config::LlmConfig {
        provider,
        api_key,
        base_url,
        model,
        reasoning_effort,
        max_tokens: update.max_tokens,
        big_context_model: normalize_optional_string(update.big_context_model).or_else(|| {
            existing
                .as_ref()
                .and_then(|config| config.big_context_model.clone())
        }),
        big_context_threshold_bytes: update.big_context_threshold_bytes.or_else(|| {
            existing
                .as_ref()
                .and_then(|config| config.big_context_threshold_bytes)
        }),
        big_context_max_tokens: update.big_context_max_tokens.or_else(|| {
            existing
                .as_ref()
                .and_then(|config| config.big_context_max_tokens)
        }),
    })
}

fn normalize_optional_string(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn is_allowed_llm_base_url(base_url: &str) -> bool {
    let lower = base_url.to_ascii_lowercase();
    if lower.starts_with("https://") {
        return true;
    }
    if !lower.starts_with("http://") {
        return false;
    }
    llm_http_host(base_url)
        .as_deref()
        .is_some_and(is_private_or_tailnet_host)
}

fn is_loopback_llm_base_url(base_url: &str) -> bool {
    llm_http_host(base_url)
        .as_deref()
        .is_some_and(|host| host == "localhost" || host == "127.0.0.1")
}

fn llm_http_host(base_url: &str) -> Option<String> {
    let rest = base_url
        .trim()
        .strip_prefix("http://")
        .or_else(|| base_url.trim().strip_prefix("HTTP://"))?;
    let authority = rest.split('/').next()?.trim();
    if authority.is_empty() {
        return None;
    }
    let host = if authority.starts_with('[') {
        authority
            .trim_start_matches('[')
            .split(']')
            .next()
            .unwrap_or_default()
            .to_string()
    } else {
        authority.split(':').next().unwrap_or_default().to_string()
    };
    let host = host.trim().trim_end_matches('.').to_ascii_lowercase();
    (!host.is_empty()).then_some(host)
}

fn is_private_or_tailnet_host(host: &str) -> bool {
    if host == "localhost" || host == "::1" || host.ends_with(".local") || host.ends_with(".ts.net")
    {
        return true;
    }
    if let Ok(ip) = host.parse::<Ipv4Addr>() {
        let octets = ip.octets();
        return octets[0] == 10
            || octets[0] == 127
            || (octets[0] == 172 && (16..=31).contains(&octets[1]))
            || (octets[0] == 192 && octets[1] == 168)
            || (octets[0] == 100 && (64..=127).contains(&octets[1]));
    }
    false
}

/// GET /api/diagnostics — Safe runtime metadata for the local chat UI.
///
/// This intentionally excludes tokens, API keys, OAuth account data, provider
/// base URLs, and repository filesystem paths unless the existing path-exposure
/// flag is enabled elsewhere. The goal is to help a local user diagnose setup
/// issues without creating a support bundle full of secrets.
async fn diagnostics_handler(State(backend): State<SharedBackend>) -> impl IntoResponse {
    let backend_guard = backend.lock().await;
    Json(diagnostics_payload(&backend_guard))
}

/// POST /api/export/pdf — render self-contained printable HTML to a real PDF.
async fn export_pdf_handler(
    Json(request): Json<PdfExportRequest>,
) -> Result<Response, (StatusCode, Json<Value>)> {
    validate_pdf_html(&request.html)
        .map_err(|message| api_error(StatusCode::BAD_REQUEST, message))?;
    let filename = sanitize_pdf_filename(request.filename.as_deref());
    let pdf_bytes = render_pdf_with_playwright(&request.html)
        .await
        .map_err(|message| {
            api_error(
                StatusCode::BAD_GATEWAY,
                format!("Export PDF natif indisponible: {message}"),
            )
        })?;

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/pdf")
        .header(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{filename}\""),
        )
        .body(Body::from(pdf_bytes))
        .map_err(|err| api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))
}

fn validate_pdf_html(html: &str) -> Result<(), String> {
    if html.trim().is_empty() {
        return Err("Le contenu HTML à exporter est vide.".to_string());
    }
    if html.len() > MAX_PDF_HTML_BYTES {
        return Err(format!(
            "Le contenu HTML est trop volumineux pour l'export PDF ({} octets, max {}).",
            html.len(),
            MAX_PDF_HTML_BYTES
        ));
    }
    Ok(())
}

async fn render_pdf_with_playwright(html: &str) -> Result<Vec<u8>, String> {
    let work_dir = native_pdf_work_dir();
    let result = async {
        tokio::fs::create_dir_all(&work_dir)
            .await
            .map_err(|err| format!("création du dossier temporaire impossible: {err}"))?;
        let html_path = work_dir.join("code-explorer-chat-export.html");
        let pdf_path = work_dir.join("code-explorer-chat-export.pdf");
        let js_path = work_dir.join("native-print-pdf.js");

        tokio::fs::write(&html_path, html)
            .await
            .map_err(|err| format!("écriture HTML impossible: {err}"))?;
        tokio::fs::write(&js_path, NATIVE_PRINT_PDF_JS)
            .await
            .map_err(|err| format!("écriture du moteur PDF impossible: {err}"))?;

        let node = find_node_binary()
            .await
            .ok_or_else(|| "Node.js introuvable. Installe Node.js puis Playwright.".to_string())?;
        let mut command = Command::new(node);
        command.arg(&js_path).arg(&html_path).arg(&pdf_path);
        if let Some(node_path) = find_global_node_modules().await {
            command.env("NODE_PATH", node_path);
        }

        let output = timeout(PDF_RENDER_TIMEOUT, command.output())
            .await
            .map_err(|_| "délai dépassé pendant le rendu Chromium.".to_string())?
            .map_err(|err| format!("lancement du moteur PDF impossible: {err}"))?;
        if !output.status.success() {
            return Err(pdf_render_error_message(&output.stdout, &output.stderr));
        }

        let bytes = tokio::fs::read(&pdf_path)
            .await
            .map_err(|err| format!("lecture du PDF impossible: {err}"))?;
        if !bytes.starts_with(b"%PDF") {
            return Err("le moteur a produit un fichier qui n'est pas un PDF valide.".to_string());
        }
        Ok(bytes)
    }
    .await;

    let _ = tokio::fs::remove_dir_all(&work_dir).await;
    result
}

fn native_pdf_work_dir() -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    std::env::temp_dir().join(format!(
        "code-explorer-native-pdf-{}-{nanos}",
        std::process::id()
    ))
}

async fn find_node_binary() -> Option<String> {
    if Command::new("node")
        .arg("--version")
        .output()
        .await
        .ok()
        .is_some_and(|output| output.status.success())
    {
        return Some("node".to_string());
    }

    #[cfg(windows)]
    {
        for path in [
            r"C:\Program Files\nodejs\node.exe",
            r"C:\Program Files (x86)\nodejs\node.exe",
        ] {
            if FsPath::new(path).exists() {
                return Some(path.to_string());
            }
        }
    }

    None
}

async fn find_global_node_modules() -> Option<String> {
    let npm = if cfg!(windows) { "npm.cmd" } else { "npm" };
    let output = timeout(
        Duration::from_secs(5),
        Command::new(npm).args(["root", "-g"]).output(),
    )
    .await
    .ok()?
    .ok()?;
    if !output.status.success() {
        return None;
    }
    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!path.is_empty() && FsPath::new(&path).exists()).then_some(path)
}

fn pdf_render_error_message(stdout: &[u8], stderr: &[u8]) -> String {
    let stderr = clean_command_output(stderr);
    let stdout = clean_command_output(stdout);
    let details = if stderr.is_empty() { stdout } else { stderr };
    if details.contains("Cannot find module 'playwright'") {
        return "Playwright est manquant. Lance `npm install -g playwright` puis `npx playwright install chromium`.".to_string();
    }
    if details.contains("Executable doesn't exist") || details.contains("browserType.launch") {
        return "Chromium Playwright est manquant. Lance `npx playwright install chromium`."
            .to_string();
    }
    if details.is_empty() {
        "le moteur PDF s'est arrêté sans diagnostic.".to_string()
    } else {
        format!("le moteur PDF a échoué: {details}")
    }
}

fn clean_command_output(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes);
    let single_line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if single_line.chars().count() > 800 {
        single_line.chars().take(800).collect::<String>() + "..."
    } else {
        single_line
    }
}

fn sanitize_pdf_filename(filename: Option<&str>) -> String {
    let raw = filename.unwrap_or("code-explorer-chat.pdf").trim();
    let without_extension = raw
        .strip_suffix(".pdf")
        .or_else(|| raw.strip_suffix(".PDF"))
        .unwrap_or(raw);
    let mut safe = without_extension
        .chars()
        .map(fold_filename_char)
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
                c
            } else {
                '-'
            }
        })
        .collect::<String>();
    while safe.contains("--") {
        safe = safe.replace("--", "-");
    }
    safe = safe.trim_matches(['-', '.']).to_string();
    if safe.is_empty() {
        safe = "code-explorer-chat".to_string();
    }
    safe.truncate(120);
    format!("{safe}.pdf")
}

fn fold_filename_char(c: char) -> char {
    match c {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'À' | 'Á' | 'Â' | 'Ã' | 'Ä' | 'Å' => 'a',
        'ç' | 'Ç' => 'c',
        'è' | 'é' | 'ê' | 'ë' | 'È' | 'É' | 'Ê' | 'Ë' => 'e',
        'ì' | 'í' | 'î' | 'ï' | 'Ì' | 'Í' | 'Î' | 'Ï' => 'i',
        'ñ' | 'Ñ' => 'n',
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'Ò' | 'Ó' | 'Ô' | 'Õ' | 'Ö' => 'o',
        'ù' | 'ú' | 'û' | 'ü' | 'Ù' | 'Ú' | 'Û' | 'Ü' => 'u',
        'ý' | 'ÿ' | 'Ý' => 'y',
        _ => c,
    }
}

fn diagnostics_payload(backend: &LocalBackend) -> Value {
    let repos = backend.registry();
    let expose_paths = expose_repo_paths();
    let generated_at_unix_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or_default();

    json!({
        "service": "code-explorer",
        "version": env!("CARGO_PKG_VERSION"),
        "generatedAtUnixMs": generated_at_unix_ms,
        "httpAuthRequired": http_auth_token().is_some(),
        "repoPathsExposed": expose_paths,
        "repos": {
            "count": repos.len(),
            "names": repos.iter().map(|entry| {
                json!({
                    "id": registry_entry_id(entry),
                    "name": entry.name,
                    "pathExposed": expose_paths,
                    "indexedAt": entry.indexed_at,
                })
            }).collect::<Vec<_>>(),
        },
        "llm": llm_config_payload(),
        "auth": {
            "chatgptOAuth": chatgpt_oauth_status_payload(),
        },
    })
}

#[derive(Debug, Deserialize)]
struct StoredChatGptAuthFile {
    pub tokens: StoredChatGptTokens,
    #[serde(default, rename = "last_refresh")]
    pub last_refresh: Option<String>,
}

#[derive(Debug, Deserialize)]
struct StoredChatGptTokens {
    #[serde(default)]
    pub id_token: String,
    #[serde(default)]
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: String,
}

fn chatgpt_auth_file_path() -> PathBuf {
    code_explorer_core::storage::repo_manager::get_global_dir()
        .join("auth")
        .join("openai.json")
}

fn chatgpt_oauth_status_payload() -> Value {
    chatgpt_oauth_status_payload_at(&chatgpt_auth_file_path())
}

fn chatgpt_oauth_status_payload_at(path: &FsPath) -> Value {
    let stored = match std::fs::read(path) {
        Ok(stored) => stored,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return json!({
                "loggedIn": false,
                "status": "missing",
                "tokenFilePresent": false,
                "tokenFileReadable": false,
                "refreshTokenPresent": false,
                "storage": "none",
            });
        }
        Err(err) => {
            return json!({
                "loggedIn": false,
                "status": "unreadable",
                "tokenFilePresent": true,
                "tokenFileReadable": false,
                "refreshTokenPresent": false,
                "storage": "unknown",
                "errorKind": format!("{:?}", err.kind()),
            });
        }
    };

    let storage = if cfg!(windows) {
        if secret_payload_needs_migration(&stored) {
            "legacy_plaintext"
        } else {
            "dpapi"
        }
    } else {
        "file"
    };

    let decoded = match decode_secret_from_storage(&stored) {
        Ok(decoded) => decoded,
        Err(_) => {
            return json!({
                "loggedIn": false,
                "status": "unreadable",
                "tokenFilePresent": true,
                "tokenFileReadable": false,
                "refreshTokenPresent": false,
                "storage": storage,
                "errorKind": "secret_storage",
            });
        }
    };

    let parsed: StoredChatGptAuthFile = match serde_json::from_slice(&decoded) {
        Ok(parsed) => parsed,
        Err(_) => {
            return json!({
                "loggedIn": false,
                "status": "invalid",
                "tokenFilePresent": true,
                "tokenFileReadable": true,
                "refreshTokenPresent": false,
                "storage": storage,
                "errorKind": "json",
            });
        }
    };

    let has_access_token = !parsed.tokens.access_token.trim().is_empty();
    let has_refresh_token = !parsed.tokens.refresh_token.trim().is_empty();
    let has_id_token = !parsed.tokens.id_token.trim().is_empty();
    let logged_in = has_access_token && has_refresh_token && has_id_token;

    json!({
        "loggedIn": logged_in,
        "status": if logged_in { "logged_in" } else { "incomplete" },
        "tokenFilePresent": true,
        "tokenFileReadable": true,
        "refreshTokenPresent": has_refresh_token,
        "lastRefresh": parsed.last_refresh,
        "storage": storage,
    })
}

/// GET /api/repos — List indexed repositories
async fn repos_handler(State(backend): State<SharedBackend>) -> impl IntoResponse {
    let backend_guard = backend.lock().await;
    let expose_paths = expose_repo_paths();
    let repos: Vec<Value> = backend_guard
        .registry()
        .iter()
        .map(|e| {
            let mut obj = json!({
                "id": registry_entry_id(e),
                "name": e.name,
                "indexedAt": e.indexed_at,
                "lastCommit": e.last_commit,
                "stats": e.stats,
                "pathExposed": expose_paths,
            });
            if expose_paths {
                obj["path"] = json!(e.path);
            }
            obj
        })
        .collect();
    Json(json!({ "repos": repos }))
}

#[derive(Deserialize)]
struct SearchParams {
    q: String,
    limit: Option<usize>,
}

/// GET /api/repos/:name/search?q=...&limit=N — Search symbols
async fn search_handler(
    State(backend): State<SharedBackend>,
    Path(name): Path<String>,
    Query(params): Query<SearchParams>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    if params.q.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Query parameter 'q' is required" })),
        ));
    }
    if params.q.len() > 1000 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Query too long (max 1000 chars)" })),
        ));
    }

    let mut backend_guard = backend.lock().await;
    let args = json!({
        "query": params.q,
        "repo": name,
        "limit": params.limit.unwrap_or(20),
    });
    match backend_guard.call_tool("query", &args).await {
        Ok(result) => Ok(Json(result)),
        Err(e) => Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": e.to_string() })),
        )),
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct FileTreeNode {
    name: String,
    path: String,
    is_dir: bool,
    children: Vec<FileTreeNode>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SourceContent {
    path: String,
    content: String,
    language: Option<String>,
    total_lines: usize,
    start_line: u32,
    end_line: u32,
    truncated: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SearchResult {
    node_id: String,
    name: String,
    label: String,
    file_path: String,
    score: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    start_line: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    end_line: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct GraphPayload {
    nodes: Vec<CytoNode>,
    edges: Vec<CytoEdge>,
    stats: GraphStats,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct GraphStats {
    node_count: usize,
    edge_count: usize,
    truncated: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct CytoNode {
    id: String,
    label: String,
    name: String,
    file_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    start_line: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    end_line: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    is_exported: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    community: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    parameter_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    return_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    layer_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    entry_point_score: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    entry_point_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    is_traced: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    trace_call_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    is_dead_candidate: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    complexity: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    depth: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct CytoEdge {
    id: String,
    source: String,
    target: String,
    rel_type: String,
    confidence: f64,
}

#[derive(Debug, Deserialize)]
struct FilesParams {
    path: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SourceParams {
    path: String,
    start: Option<u32>,
    end: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct SymbolsParams {
    q: String,
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct GraphParams {
    zoom: Option<String>,
    labels: Option<String>,
    #[serde(default, alias = "filePath")]
    file_path: Option<String>,
    max_nodes: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct NeighborhoodParams {
    #[serde(alias = "nodeId")]
    node_id: String,
    depth: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct PdfExportRequest {
    html: String,
    filename: Option<String>,
}

const MAX_SYMBOL_QUERY_CHARS: usize = 1000;
const MAX_SYMBOL_RESULTS: usize = 100;
const MAX_GRAPH_NODES: usize = 500;
const DEFAULT_GRAPH_NODES: usize = 200;
const MAX_NEIGHBORHOOD_DEPTH: u32 = 5;
const MAX_NEIGHBORHOOD_NODES: usize = 500;
const MAX_SOURCE_FILE_BYTES: u64 = 1_000_000;
const MAX_SOURCE_LINES: u32 = 2_000;
const MAX_PDF_HTML_BYTES: usize = 8 * 1024 * 1024;
const PDF_RENDER_TIMEOUT: Duration = Duration::from_secs(90);

fn api_error(status: StatusCode, message: impl Into<String>) -> (StatusCode, Json<Value>) {
    (status, Json(json!({ "error": message.into() })))
}

fn map_backend_error(error: impl std::fmt::Display) -> (StatusCode, Json<Value>) {
    api_error(StatusCode::BAD_REQUEST, error.to_string())
}

fn clamp_query_limit(limit: Option<usize>, default: usize, max: usize) -> usize {
    limit.unwrap_or(default).clamp(1, max)
}

fn normalize_repo_relative_path(raw: &str) -> Result<String, String> {
    let normalized = raw.trim().replace('\\', "/");
    if normalized.is_empty() {
        return Ok(String::new());
    }
    if normalized.contains('\0') {
        return Err("Path contains a NUL byte.".to_string());
    }

    let path = FsPath::new(&normalized);
    if path.is_absolute() {
        return Err("Absolute paths are not allowed.".to_string());
    }

    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => {
                let value = part
                    .to_str()
                    .ok_or_else(|| "Path must be valid UTF-8.".to_string())?;
                if !value.is_empty() {
                    parts.push(value.to_string());
                }
            }
            Component::CurDir => {}
            Component::ParentDir => {
                return Err("Path traversal is not allowed.".to_string());
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err("Absolute paths are not allowed.".to_string());
            }
        }
    }

    Ok(parts.join("/"))
}

fn safe_repo_file_path(repo_root: &FsPath, relative_path: &str) -> Result<PathBuf, String> {
    let relative_path = normalize_repo_relative_path(relative_path)?;
    if relative_path.is_empty() {
        return Err("Query parameter 'path' is required.".to_string());
    }

    let repo_canonical = repo_root
        .canonicalize()
        .map_err(|e| format!("Failed to resolve repository path: {e}"))?;
    let full_path = repo_root.join(&relative_path);
    let file_canonical = full_path
        .canonicalize()
        .map_err(|e| format!("Failed to resolve file '{relative_path}': {e}"))?;
    if !file_canonical.starts_with(&repo_canonical) {
        return Err("Access denied: path is outside the repository.".to_string());
    }
    if !file_canonical.is_file() {
        return Err(format!("'{relative_path}' is not a file."));
    }

    Ok(file_canonical)
}

fn language_for_path(path: &str) -> Option<String> {
    FsPath::new(path)
        .extension()
        .and_then(|ext| ext.to_str())
        .and_then(|ext| SupportedLanguage::from_extension(&format!(".{ext}")))
        .map(|language| language.as_str().to_string())
}

fn normalize_graph_file_path(path: &str) -> String {
    path.replace('\\', "/")
}

fn basename(path: &str) -> String {
    path.rsplit('/').next().unwrap_or(path).to_string()
}

fn collect_file_paths(graph: &code_explorer_core::graph::KnowledgeGraph) -> Vec<String> {
    let mut paths: Vec<String> = graph
        .iter_nodes()
        .filter(|node| node.label == NodeLabel::File)
        .map(|node| normalize_graph_file_path(&node.properties.file_path))
        .filter(|path| !path.is_empty())
        .collect();
    paths.sort();
    paths.dedup();
    paths
}

fn file_paths_for_prefix(paths: &[String], prefix: &str) -> Vec<String> {
    if prefix.is_empty() {
        return paths.to_vec();
    }

    if paths.iter().any(|path| path == prefix) {
        return vec![basename(prefix)];
    }

    let prefix_with_slash = format!("{prefix}/");
    paths
        .iter()
        .filter_map(|path| {
            path.strip_prefix(&prefix_with_slash)
                .filter(|rest| !rest.is_empty())
                .map(|rest| rest.to_string())
        })
        .collect()
}

fn build_tree_from_paths(paths: &[String], prefix: &str) -> Vec<FileTreeNode> {
    let refs: Vec<&str> = paths.iter().map(String::as_str).collect();
    build_tree_impl(&refs, prefix)
}

fn build_tree_impl(paths: &[&str], prefix: &str) -> Vec<FileTreeNode> {
    let mut dir_children: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut local_files: Vec<String> = Vec::new();

    for path in paths {
        let parts: Vec<&str> = path.splitn(2, '/').collect();
        if parts.len() > 1 {
            dir_children
                .entry(parts[0].to_string())
                .or_default()
                .push(parts[1].to_string());
        } else if !path.is_empty() {
            local_files.push(path.to_string());
        }
    }

    let join = |base: &str, name: &str| -> String {
        if base.is_empty() {
            name.to_string()
        } else {
            format!("{base}/{name}")
        }
    };

    let mut result = Vec::new();
    for (dir_name, child_paths) in &dir_children {
        let child_refs: Vec<&str> = child_paths.iter().map(String::as_str).collect();
        let dir_path = join(prefix, dir_name);
        result.push(FileTreeNode {
            name: dir_name.clone(),
            path: dir_path.clone(),
            is_dir: true,
            children: build_tree_impl(&child_refs, &dir_path),
        });
    }

    for file_name in &local_files {
        result.push(FileTreeNode {
            name: file_name.clone(),
            path: join(prefix, file_name),
            is_dir: false,
            children: Vec::new(),
        });
    }

    result
}

/// GET /api/repos/:name/symbols?q=...&limit=N — Browser-native symbol search.
async fn symbols_handler(
    State(backend): State<SharedBackend>,
    Path(name): Path<String>,
    Query(params): Query<SymbolsParams>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    if params.q.trim().is_empty() {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "Query parameter 'q' is required.",
        ));
    }
    if params.q.chars().count() > MAX_SYMBOL_QUERY_CHARS {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            format!("Query too long (max {MAX_SYMBOL_QUERY_CHARS} chars)."),
        ));
    }

    let limit = clamp_query_limit(params.limit, 20, MAX_SYMBOL_RESULTS);
    let mut backend_guard = backend.lock().await;
    let (_entry, graph, _indexes, fts) = backend_guard
        .load_repo_indexes(&name)
        .map_err(map_backend_error)?;
    let mut results = fts.search(&graph, params.q.trim(), None, limit);
    results.truncate(limit);

    let symbols: Vec<SearchResult> = results
        .into_iter()
        .map(|result| SearchResult {
            node_id: result.node_id,
            name: result.name,
            label: result.label,
            file_path: result.file_path,
            score: result.score,
            start_line: result.start_line,
            end_line: result.end_line,
        })
        .collect();

    Ok(Json(json!({ "symbols": symbols })))
}

/// GET /api/repos/:name/files?path=... — File tree for an indexed repository.
async fn files_handler(
    State(backend): State<SharedBackend>,
    Path(name): Path<String>,
    Query(params): Query<FilesParams>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let prefix = params
        .path
        .as_deref()
        .map(normalize_repo_relative_path)
        .transpose()
        .map_err(|e| api_error(StatusCode::BAD_REQUEST, e))?
        .unwrap_or_default();

    let mut backend_guard = backend.lock().await;
    let (_entry, graph, _indexes, _fts) = backend_guard
        .load_repo_indexes(&name)
        .map_err(map_backend_error)?;
    let paths = collect_file_paths(&graph);
    let tree = if !prefix.is_empty() && paths.iter().any(|path| path == &prefix) {
        vec![FileTreeNode {
            name: basename(&prefix),
            path: prefix.clone(),
            is_dir: false,
            children: Vec::new(),
        }]
    } else {
        let scoped_paths = file_paths_for_prefix(&paths, &prefix);
        build_tree_from_paths(&scoped_paths, &prefix)
    };

    Ok(Json(json!({
        "path": prefix,
        "files": tree,
    })))
}

/// GET /api/repos/:name/source?path=...&start=1&end=80 — Safe source preview.
async fn source_handler(
    State(backend): State<SharedBackend>,
    Path(name): Path<String>,
    Query(params): Query<SourceParams>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let relative_path = normalize_repo_relative_path(&params.path)
        .map_err(|e| api_error(StatusCode::BAD_REQUEST, e))?;
    if relative_path.is_empty() {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "Query parameter 'path' is required.",
        ));
    }

    let backend_guard = backend.lock().await;
    let entry = backend_guard
        .resolve_repo(Some(&name))
        .map_err(map_backend_error)?
        .clone();
    drop(backend_guard);

    let repo_root = FsPath::new(&entry.path);
    let safe_path = safe_repo_file_path(repo_root, &relative_path)
        .map_err(|e| api_error(StatusCode::BAD_REQUEST, e))?;
    let metadata = std::fs::metadata(&safe_path).map_err(|e| {
        api_error(
            StatusCode::BAD_REQUEST,
            format!("Failed to read metadata for '{relative_path}': {e}"),
        )
    })?;
    if metadata.len() > MAX_SOURCE_FILE_BYTES {
        return Err(api_error(
            StatusCode::PAYLOAD_TOO_LARGE,
            format!(
                "File is too large for preview ({} bytes, max {}).",
                metadata.len(),
                MAX_SOURCE_FILE_BYTES
            ),
        ));
    }

    let content = std::fs::read_to_string(&safe_path).map_err(|e| {
        api_error(
            StatusCode::BAD_REQUEST,
            format!("Failed to read file '{relative_path}': {e}"),
        )
    })?;
    let lines: Vec<&str> = content.lines().collect();
    let total_lines = lines.len();

    if total_lines == 0 {
        return Ok(Json(json!(SourceContent {
            path: relative_path.clone(),
            content: String::new(),
            language: language_for_path(&relative_path),
            total_lines,
            start_line: 0,
            end_line: 0,
            truncated: false,
        })));
    }

    let requested_start = params.start.unwrap_or(1);
    if requested_start == 0 {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "Line range start must be >= 1.",
        ));
    }

    let requested_end = params.end.unwrap_or_else(|| {
        if params.start.is_some() {
            requested_start.saturating_add(MAX_SOURCE_LINES - 1)
        } else {
            total_lines as u32
        }
    });
    if requested_end < requested_start {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "Line range end must be >= start.",
        ));
    }

    let requested_count = requested_end
        .saturating_sub(requested_start)
        .saturating_add(1);
    let capped_count = requested_count.min(MAX_SOURCE_LINES);
    let effective_end = requested_start.saturating_add(capped_count.saturating_sub(1));
    let start_idx = requested_start.saturating_sub(1) as usize;
    let end_idx = (effective_end as usize).min(total_lines);
    let snippet = if start_idx >= total_lines {
        String::new()
    } else {
        lines[start_idx..end_idx].join("\n")
    };

    let payload = SourceContent {
        path: relative_path.clone(),
        content: snippet,
        language: language_for_path(&relative_path),
        total_lines,
        start_line: requested_start,
        end_line: effective_end.min(total_lines as u32),
        truncated: requested_count > capped_count,
    };

    Ok(Json(json!(payload)))
}

/// GET /api/repos/:name/graph?zoom=symbol&max_nodes=200 — Bounded graph payload.
async fn graph_handler(
    State(backend): State<SharedBackend>,
    Path(name): Path<String>,
    Query(params): Query<GraphParams>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let zoom = parse_zoom_level(params.zoom.as_deref())
        .map_err(|e| api_error(StatusCode::BAD_REQUEST, e))?;
    let max_nodes = clamp_query_limit(params.max_nodes, DEFAULT_GRAPH_NODES, MAX_GRAPH_NODES);
    let labels = parse_label_filter(params.labels.as_deref());
    let file_path = params
        .file_path
        .as_deref()
        .map(normalize_repo_relative_path)
        .transpose()
        .map_err(|e| api_error(StatusCode::BAD_REQUEST, e))?;

    let mut backend_guard = backend.lock().await;
    let (_entry, graph, indexes, _fts) = backend_guard
        .load_repo_indexes(&name)
        .map_err(map_backend_error)?;
    let payload = build_graph_payload(&graph, &indexes, zoom, labels, file_path, max_nodes);
    Ok(Json(json!(payload)))
}

/// GET /api/repos/:name/graph/neighborhood?node_id=...&depth=2
async fn graph_neighborhood_handler(
    State(backend): State<SharedBackend>,
    Path(name): Path<String>,
    Query(params): Query<NeighborhoodParams>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    if params.node_id.trim().is_empty() {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "Query parameter 'node_id' is required.",
        ));
    }

    let depth = params.depth.unwrap_or(2).min(MAX_NEIGHBORHOOD_DEPTH);
    let mut backend_guard = backend.lock().await;
    let (_entry, graph, indexes, _fts) = backend_guard
        .load_repo_indexes(&name)
        .map_err(map_backend_error)?;
    if graph.get_node(&params.node_id).is_none() {
        return Err(api_error(
            StatusCode::NOT_FOUND,
            format!("Node '{}' not found.", params.node_id),
        ));
    }

    let payload = build_neighborhood_payload(&graph, &indexes, &params.node_id, depth);
    Ok(Json(json!(payload)))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ApiZoomLevel {
    Package,
    Module,
    Symbol,
}

fn parse_zoom_level(value: Option<&str>) -> Result<ApiZoomLevel, String> {
    match value.unwrap_or("symbol").trim().to_lowercase().as_str() {
        "package" | "packages" => Ok(ApiZoomLevel::Package),
        "module" | "modules" | "file" | "files" => Ok(ApiZoomLevel::Module),
        "symbol" | "symbols" => Ok(ApiZoomLevel::Symbol),
        other => Err(format!(
            "Unsupported zoom level '{other}'. Expected package, module, or symbol."
        )),
    }
}

fn parse_label_filter(value: Option<&str>) -> Option<Vec<NodeLabel>> {
    let labels: Vec<NodeLabel> = value
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|label| !label.is_empty())
        .filter_map(NodeLabel::from_str_label)
        .collect();
    if labels.is_empty() {
        None
    } else {
        Some(labels)
    }
}

fn labels_for_zoom(zoom: ApiZoomLevel) -> Vec<NodeLabel> {
    match zoom {
        ApiZoomLevel::Package => vec![NodeLabel::Folder, NodeLabel::Package, NodeLabel::Project],
        ApiZoomLevel::Module => vec![NodeLabel::File, NodeLabel::Module, NodeLabel::Folder],
        ApiZoomLevel::Symbol => vec![
            NodeLabel::Function,
            NodeLabel::Class,
            NodeLabel::Method,
            NodeLabel::Interface,
            NodeLabel::Struct,
            NodeLabel::Trait,
            NodeLabel::Enum,
            NodeLabel::Variable,
            NodeLabel::Type,
            NodeLabel::Const,
            NodeLabel::Constructor,
            NodeLabel::Property,
            NodeLabel::Namespace,
            NodeLabel::Route,
            NodeLabel::Tool,
            NodeLabel::Controller,
            NodeLabel::ControllerAction,
            NodeLabel::Service,
            NodeLabel::Repository,
            NodeLabel::View,
            NodeLabel::DbEntity,
            NodeLabel::DbContext,
        ],
    }
}

fn relationships_for_zoom(zoom: ApiZoomLevel) -> Vec<RelationshipType> {
    match zoom {
        ApiZoomLevel::Package => vec![RelationshipType::Contains],
        ApiZoomLevel::Module => vec![RelationshipType::Contains, RelationshipType::Imports],
        ApiZoomLevel::Symbol => vec![
            RelationshipType::Calls,
            RelationshipType::Uses,
            RelationshipType::Imports,
            RelationshipType::Inherits,
            RelationshipType::Implements,
            RelationshipType::Extends,
            RelationshipType::HasMethod,
            RelationshipType::HasAction,
            RelationshipType::CallsService,
            RelationshipType::CallsAction,
            RelationshipType::RendersView,
            RelationshipType::MapsToEntity,
        ],
    }
}

fn file_filter_matches(node_file_path: &str, filter: &str) -> bool {
    if filter.is_empty() {
        return true;
    }
    let node_path = normalize_graph_file_path(node_file_path);
    node_path == filter || node_path.starts_with(&format!("{filter}/"))
}

fn build_graph_payload(
    graph: &code_explorer_core::graph::KnowledgeGraph,
    indexes: &code_explorer_db::inmemory::cypher::GraphIndexes,
    zoom: ApiZoomLevel,
    labels: Option<Vec<NodeLabel>>,
    file_path: Option<String>,
    max_nodes: usize,
) -> GraphPayload {
    let allowed_labels = labels.unwrap_or_else(|| labels_for_zoom(zoom));
    let allowed_relationships = relationships_for_zoom(zoom);
    let file_path = file_path.unwrap_or_default();

    let mut scored_nodes: Vec<(f64, &GraphNode)> = graph
        .iter_nodes()
        .filter(|node| allowed_labels.contains(&node.label))
        .filter(|node| file_filter_matches(&node.properties.file_path, &file_path))
        .map(|node| (node_importance_score(node, indexes), node))
        .collect();
    scored_nodes.sort_by(|a, b| b.0.total_cmp(&a.0));

    let total_candidates = scored_nodes.len();
    let nodes: Vec<CytoNode> = scored_nodes
        .into_iter()
        .take(max_nodes)
        .map(|(_, node)| node_to_cyto(node, None))
        .collect();
    let node_ids: HashSet<&str> = nodes.iter().map(|node| node.id.as_str()).collect();

    let edges: Vec<CytoEdge> = graph
        .iter_relationships()
        .filter(|rel| allowed_relationships.contains(&rel.rel_type))
        .filter(|rel| {
            node_ids.contains(rel.source_id.as_str()) && node_ids.contains(rel.target_id.as_str())
        })
        .map(rel_to_cyto)
        .collect();

    GraphPayload {
        stats: GraphStats {
            node_count: nodes.len(),
            edge_count: edges.len(),
            truncated: total_candidates > nodes.len(),
        },
        nodes,
        edges,
    }
}

fn build_neighborhood_payload(
    graph: &code_explorer_core::graph::KnowledgeGraph,
    indexes: &code_explorer_db::inmemory::cypher::GraphIndexes,
    center_node_id: &str,
    max_depth: u32,
) -> GraphPayload {
    let mut visited = HashSet::new();
    let mut depth_map = std::collections::HashMap::new();
    let mut queue = std::collections::VecDeque::new();
    let mut hit_cap = false;

    visited.insert(center_node_id.to_string());
    depth_map.insert(center_node_id.to_string(), 0u32);
    queue.push_back((center_node_id.to_string(), 0u32));

    'bfs: while let Some((node_id, depth)) = queue.pop_front() {
        if depth >= max_depth {
            continue;
        }

        if let Some(outgoing) = indexes.outgoing.get(&node_id) {
            for (target, _) in outgoing {
                if visited.len() >= MAX_NEIGHBORHOOD_NODES {
                    hit_cap = true;
                    break 'bfs;
                }
                if visited.insert(target.clone()) {
                    depth_map.insert(target.clone(), depth + 1);
                    queue.push_back((target.clone(), depth + 1));
                }
            }
        }

        if let Some(incoming) = indexes.incoming.get(&node_id) {
            for (source, _) in incoming {
                if visited.len() >= MAX_NEIGHBORHOOD_NODES {
                    hit_cap = true;
                    break 'bfs;
                }
                if visited.insert(source.clone()) {
                    depth_map.insert(source.clone(), depth + 1);
                    queue.push_back((source.clone(), depth + 1));
                }
            }
        }
    }

    let nodes: Vec<CytoNode> = visited
        .iter()
        .filter_map(|id| {
            graph
                .get_node(id)
                .map(|node| node_to_cyto(node, depth_map.get(id).copied()))
        })
        .collect();
    let node_ids: HashSet<&str> = nodes.iter().map(|node| node.id.as_str()).collect();
    let edges: Vec<CytoEdge> = graph
        .iter_relationships()
        .filter(|rel| {
            node_ids.contains(rel.source_id.as_str()) && node_ids.contains(rel.target_id.as_str())
        })
        .map(rel_to_cyto)
        .collect();

    GraphPayload {
        stats: GraphStats {
            node_count: nodes.len(),
            edge_count: edges.len(),
            truncated: hit_cap,
        },
        nodes,
        edges,
    }
}

fn node_importance_score(
    node: &GraphNode,
    indexes: &code_explorer_db::inmemory::cypher::GraphIndexes,
) -> f64 {
    let indegree = indexes.incoming.get(node.id.as_str()).map_or(0, Vec::len);
    let outdegree = indexes.outgoing.get(node.id.as_str()).map_or(0, Vec::len);

    let mut score = (indegree + outdegree) as f64 * 2.0;
    if let Some(entry_point_score) = node.properties.entry_point_score {
        score += entry_point_score * 10.0;
    }
    if node.properties.is_exported == Some(true) {
        score += 5.0;
    }
    if node.properties.is_traced == Some(true) {
        score += 3.0;
    }
    match node.label {
        NodeLabel::Controller | NodeLabel::Service => score += 20.0,
        NodeLabel::Class | NodeLabel::Interface => score += 10.0,
        NodeLabel::Module | NodeLabel::Package => score += 15.0,
        _ => {}
    }
    score
}

fn node_to_cyto(node: &GraphNode, depth: Option<u32>) -> CytoNode {
    CytoNode {
        id: node.id.clone(),
        label: node.label.as_str().to_string(),
        name: node.properties.name.clone(),
        file_path: normalize_graph_file_path(&node.properties.file_path),
        start_line: node.properties.start_line,
        end_line: node.properties.end_line,
        is_exported: node.properties.is_exported,
        community: node.properties.heuristic_label.clone(),
        language: node
            .properties
            .language
            .map(|language| language.as_str().to_string()),
        description: node.properties.description.clone(),
        parameter_count: node.properties.parameter_count,
        return_type: node.properties.return_type.clone(),
        layer_type: node.properties.layer_type.clone(),
        entry_point_score: node.properties.entry_point_score,
        entry_point_reason: node.properties.entry_point_reason.clone(),
        is_traced: node.properties.is_traced,
        trace_call_count: node.properties.trace_call_count,
        is_dead_candidate: node.properties.is_dead_candidate,
        complexity: node.properties.complexity,
        depth,
    }
}

fn rel_to_cyto(rel: &GraphRelationship) -> CytoEdge {
    CytoEdge {
        id: rel.id.clone(),
        source: rel.source_id.clone(),
        target: rel.target_id.clone(),
        rel_type: rel.rel_type.as_str().to_string(),
        confidence: rel.confidence,
    }
}

/// GET /api/repos/:name/stats — Repository statistics
async fn stats_handler(
    State(backend): State<SharedBackend>,
    Path(name): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let backend_guard = backend.lock().await;
    let resource = crate::resources::read_resource(
        &format!("code-explorer://repos/{}/stats", name),
        backend_guard.registry(),
    );
    match resource {
        Some(r) => Ok(Json(r)),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(json!({ "error": format!("Repository '{}' not found", name) })),
        )),
    }
}

#[derive(Deserialize)]
struct HotspotsParams {
    days: Option<u32>,
    limit: Option<usize>,
}

/// GET /api/repos/:name/hotspots?days=90&limit=20 — File hotspots
async fn hotspots_handler(
    State(backend): State<SharedBackend>,
    Path(name): Path<String>,
    Query(params): Query<HotspotsParams>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let mut backend_guard = backend.lock().await;
    let args = json!({
        "repo": name,
        "since_days": params.days.unwrap_or(90),
        "limit": params.limit.unwrap_or(20),
    });
    match backend_guard.call_tool("hotspots", &args).await {
        Ok(result) => Ok(Json(result)),
        Err(e) => Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": e.to_string() })),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn test_router_creation() {
        let _router = mcp_http_router();
        // Router creation should not panic
    }

    #[test]
    fn test_http_auth_token_is_trimmed_and_optional() {
        let _guard = ENV_LOCK.lock().unwrap();
        std::env::remove_var("CODE_EXPLORER_HTTP_TOKEN");
        assert!(http_auth_token().is_none());

        std::env::set_var("CODE_EXPLORER_HTTP_TOKEN", "  secret-token  ");
        assert_eq!(
            http_auth_token().as_deref().map(|s| s.as_str()),
            Some("secret-token")
        );
        std::env::remove_var("CODE_EXPLORER_HTTP_TOKEN");
    }

    #[test]
    fn test_expose_repo_paths_defaults_to_false() {
        let _guard = ENV_LOCK.lock().unwrap();
        std::env::remove_var("CODE_EXPLORER_EXPOSE_REPO_PATHS");
        assert!(!expose_repo_paths());

        std::env::set_var("CODE_EXPLORER_EXPOSE_REPO_PATHS", "true");
        assert!(expose_repo_paths());
        std::env::remove_var("CODE_EXPLORER_EXPOSE_REPO_PATHS");
    }

    #[test]
    fn test_diagnostics_payload_omits_secrets_and_paths_by_default() {
        let _guard = ENV_LOCK.lock().unwrap();
        std::env::remove_var("CODE_EXPLORER_EXPOSE_REPO_PATHS");
        std::env::remove_var("CODE_EXPLORER_HTTP_TOKEN");

        let backend = LocalBackend::new();
        let payload = diagnostics_payload(&backend);
        let text = serde_json::to_string(&payload).unwrap();

        assert_eq!(payload["service"], "code-explorer");
        assert_eq!(payload["repoPathsExposed"], false);
        assert_eq!(payload["httpAuthRequired"], false);
        assert!(!text.contains("api_key"));
        assert!(!text.contains("access_token"));
        assert!(!text.contains("refresh_token"));
    }

    #[test]
    fn pdf_export_validation_rejects_empty_and_oversized_html() {
        assert!(validate_pdf_html("<html><body>OK</body></html>").is_ok());
        assert!(validate_pdf_html("   ").unwrap_err().contains("vide"));

        let oversized = "x".repeat(MAX_PDF_HTML_BYTES + 1);
        assert!(validate_pdf_html(&oversized)
            .unwrap_err()
            .contains("trop volumineux"));
    }

    #[test]
    fn pdf_export_filename_is_ascii_safe_and_has_pdf_extension() {
        assert_eq!(
            sanitize_pdf_filename(Some("Sample v2 / Démo finale.pdf")),
            "Sample-v2-Demo-finale.pdf"
        );
        assert_eq!(sanitize_pdf_filename(Some("../secret")), "secret.pdf");
        assert_eq!(sanitize_pdf_filename(Some("   ")), "code-explorer-chat.pdf");
    }

    #[test]
    fn llm_config_update_accepts_local_openai_compatible_provider() {
        let config = normalize_llm_config_update(LlmConfigUpdate {
            provider: "ollama".to_string(),
            api_key: None,
            base_url: "http://localhost:11434/v1/".to_string(),
            model: "local-chat-model:latest".to_string(),
            reasoning_effort: Some("none".to_string()),
            max_tokens: Some(4096),
            big_context_model: None,
            big_context_threshold_bytes: None,
            big_context_max_tokens: None,
        })
        .expect("valid local provider");

        assert_eq!(config.provider, "ollama");
        assert_eq!(config.base_url, "http://localhost:11434/v1");
        assert_eq!(config.api_key, "");
        assert_eq!(config.reasoning_effort.as_deref(), Some("none"));
    }

    #[test]
    fn llm_config_update_accepts_darkstar_tailnet_without_api_key() {
        let config = normalize_llm_config_update(LlmConfigUpdate {
            provider: "darkstar-ollama".to_string(),
            api_key: None,
            base_url: "http://100.73.222.64:11434/v1".to_string(),
            model: "tailnet-chat-model:latest".to_string(),
            reasoning_effort: Some("none".to_string()),
            max_tokens: Some(8192),
            big_context_model: None,
            big_context_threshold_bytes: None,
            big_context_max_tokens: None,
        })
        .expect("valid tailnet provider");

        assert_eq!(config.provider, "darkstar-ollama");
        assert_eq!(config.base_url, "http://100.73.222.64:11434/v1");
        assert_eq!(config.api_key, "");
    }

    #[test]
    fn llm_config_update_rejects_api_key_on_non_loopback_http() {
        let err = normalize_llm_config_update(LlmConfigUpdate {
            provider: "darkstar-ollama".to_string(),
            api_key: Some("secret".to_string()),
            base_url: "http://100.73.222.64:11434/v1".to_string(),
            model: "tailnet-chat-model:latest".to_string(),
            reasoning_effort: Some("none".to_string()),
            max_tokens: Some(8192),
            big_context_model: None,
            big_context_threshold_bytes: None,
            big_context_max_tokens: None,
        })
        .expect_err("remote HTTP key should be rejected");

        assert!(err.contains("clé API"));
    }

    #[test]
    fn llm_config_update_rejects_non_local_http_provider() {
        let err = normalize_llm_config_update(LlmConfigUpdate {
            provider: "openai".to_string(),
            api_key: Some("secret".to_string()),
            base_url: "http://api.example.com/v1".to_string(),
            model: "gpt-4o-mini".to_string(),
            reasoning_effort: Some("medium".to_string()),
            max_tokens: Some(4096),
            big_context_model: None,
            big_context_threshold_bytes: None,
            big_context_max_tokens: None,
        })
        .expect_err("remote HTTP should be rejected");

        assert!(err.contains("HTTPS"));
    }

    #[test]
    fn llm_models_parser_keeps_chat_models_and_skips_embeddings() {
        let payload = json!({
            "object": "list",
            "data": [
                { "id": "code-chat-model:latest" },
                { "id": "nomic-embed-text:latest" },
                { "id": "general-chat-model:latest" },
                { "object": "model" }
            ]
        });

        let models = parse_openai_model_list(&payload);

        assert_eq!(
            models,
            vec!["code-chat-model:latest", "general-chat-model:latest"]
        );
    }

    #[test]
    fn llm_test_preview_accepts_reasoning_only_local_responses() {
        let payload = json!({
            "choices": [
                {
                    "message": {
                        "role": "assistant",
                        "content": "",
                        "reasoning": "The model thought before producing visible text."
                    }
                }
            ]
        });

        let preview = llm_test_response_preview(&payload).expect("valid reasoning response");

        assert_eq!(
            preview,
            "Réponse reçue (raisonnement interne sans contenu final visible)."
        );
    }

    #[test]
    fn tailscale_status_parser_keeps_online_and_offline_hosts() {
        let payload = r#"{
            "Self": {
                "HostName": "MINISTAR",
                "DNSName": "ministar.tail.example.",
                "OS": "windows",
                "TailscaleIPs": ["100.90.108.4"],
                "Online": true,
                "Active": false
            },
            "Peer": {
                "nodekey:darkstar": {
                    "HostName": "DARKSTAR",
                    "DNSName": "darkstar.tail.example.",
                    "OS": "windows",
                    "TailscaleIPs": ["100.73.222.64"],
                    "Online": false,
                    "Active": false
                },
                "nodekey:ministar-linux": {
                    "HostName": "ministar-linux",
                    "DNSName": "ministar-linux.tail.example.",
                    "OS": "linux",
                    "TailscaleIPs": ["100.98.18.76"],
                    "Online": true,
                    "Active": true
                }
            }
        }"#;

        let hosts = parse_tailscale_status_hosts(payload).expect("valid tailscale status");

        assert_eq!(hosts.len(), 3);
        let ministar = hosts
            .iter()
            .find(|host| host["hostName"] == "ministar-linux")
            .expect("ministar-linux host");
        assert_eq!(ministar["online"], true);
        assert_eq!(ministar["active"], true);
        assert_eq!(ministar["tailscaleIp"], "100.98.18.76");

        let darkstar = hosts
            .iter()
            .find(|host| host["hostName"] == "DARKSTAR")
            .expect("darkstar host");
        assert_eq!(darkstar["online"], false);
    }

    #[test]
    fn test_chatgpt_oauth_status_missing_is_safe() {
        let path = unique_temp_auth_path("missing");
        let payload = chatgpt_oauth_status_payload_at(&path);
        let text = serde_json::to_string(&payload).unwrap();

        assert_eq!(payload["loggedIn"], false);
        assert_eq!(payload["status"], "missing");
        assert_eq!(payload["tokenFilePresent"], false);
        assert!(!text.contains("openai.json"));
        assert!(!text.contains("access_token"));
        assert!(!text.contains("refresh_token"));
    }

    #[test]
    fn test_chatgpt_oauth_status_valid_file_omits_token_values() {
        let path = unique_temp_auth_path("valid");
        let raw = json!({
            "tokens": {
                "id_token": "secret-id-token",
                "access_token": "secret-access-token",
                "refresh_token": "secret-refresh-token"
            },
            "last_refresh": "2026-05-06T20:00:00Z"
        })
        .to_string();
        std::fs::write(&path, raw).unwrap();

        let payload = chatgpt_oauth_status_payload_at(&path);
        let text = serde_json::to_string(&payload).unwrap();
        let _ = std::fs::remove_file(&path);

        assert_eq!(payload["loggedIn"], true);
        assert_eq!(payload["status"], "logged_in");
        assert_eq!(payload["refreshTokenPresent"], true);
        assert_eq!(payload["lastRefresh"], "2026-05-06T20:00:00Z");
        assert!(!text.contains("secret-id-token"));
        assert!(!text.contains("secret-access-token"));
        assert!(!text.contains("secret-refresh-token"));
    }

    #[test]
    fn repo_relative_paths_are_normalized_without_traversal() {
        assert_eq!(
            normalize_repo_relative_path("src\\commands\\serve.rs").unwrap(),
            "src/commands/serve.rs"
        );
        assert_eq!(
            normalize_repo_relative_path("./src/./main.rs").unwrap(),
            "src/main.rs"
        );
        assert!(normalize_repo_relative_path("../secrets.txt").is_err());
        assert!(normalize_repo_relative_path("src/../../secrets.txt").is_err());
        assert!(normalize_repo_relative_path("/tmp/secrets.txt").is_err());
    }

    #[cfg(windows)]
    #[test]
    fn repo_relative_paths_reject_windows_absolute_paths() {
        assert!(normalize_repo_relative_path("C:\\Users\\patri\\secret.txt").is_err());
    }

    #[test]
    fn tree_builder_keeps_full_paths_under_requested_prefix() {
        let paths = vec![
            "Controllers/CourrierController.cs".to_string(),
            "Controllers/HomeController.cs".to_string(),
            "Models/Courrier.cs".to_string(),
        ];
        let scoped = file_paths_for_prefix(&paths, "Controllers");
        let tree = build_tree_from_paths(&scoped, "Controllers");

        assert_eq!(tree.len(), 2);
        assert_eq!(tree[0].name, "CourrierController.cs");
        assert_eq!(tree[0].path, "Controllers/CourrierController.cs");
        assert!(!tree[0].is_dir);
    }

    #[test]
    fn graph_query_parses_zoom_levels_and_rejects_unknown_values() {
        assert_eq!(parse_zoom_level(None).unwrap(), ApiZoomLevel::Symbol);
        assert_eq!(
            parse_zoom_level(Some("modules")).unwrap(),
            ApiZoomLevel::Module
        );
        assert!(parse_zoom_level(Some("everything")).is_err());
    }

    fn unique_temp_auth_path(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        std::env::temp_dir().join(format!(
            "code-explorer-chatgpt-auth-status-{label}-{}-{nanos}.json",
            std::process::id()
        ))
    }
}

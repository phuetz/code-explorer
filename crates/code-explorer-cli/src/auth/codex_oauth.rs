//! ChatGPT Codex OAuth Authorization Code + PKCE flow.
//!
//! This module implements the same login dance the official `openai/codex`
//! Rust CLI uses, ported to a single self-contained file so we don't have
//! to drag in `codex-app-server-protocol`, `codex-config`,
//! `codex-utils-template`, etc. The flow:
//!
//! 1. Generate a 64-byte random `code_verifier`, base64url-encoded.
//! 2. SHA-256 it and base64url-encode the digest → `code_challenge` (S256).
//! 3. Spin up an `axum` callback server on `127.0.0.1:1455` or, if that
//!    registered Codex redirect port is busy, `127.0.0.1:1457`.
//! 4. Open `https://auth.openai.com/oauth/authorize?...` in the user's
//!    browser. They're already signed in to ChatGPT, so it's one click.
//! 5. The browser redirects to `http://localhost:<actual_port>/auth/callback?code=...`.
//! 6. Server exchanges the code for `{id_token, access_token, refresh_token}`
//!    by POSTing to `https://auth.openai.com/oauth/token`.
//! 7. Tokens land on disk under `<CODE_EXPLORER_HOME>/.codeexplorer/auth/openai.json`,
//!    protected with Windows DPAPI on Windows and restrictive permissions on
//!    Unix-like systems.
//!
//! Once stored, [`get_chatgpt_auth`] returns a usable bearer token plus the
//! ChatGPT account metadata needed by the Codex Responses backend, doing an
//! opportunistic refresh when the cached token is older than an hour.

use std::fs::OpenOptions;
use std::io::Write;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

use anyhow::{Context, Result};
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse};
use axum::routing::get;
use axum::Router;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chrono::Utc;
use code_explorer_core::llm::sanitize_llm_error_body;
use code_explorer_core::secret_store::{
    decode_secret_from_storage, encode_secret_for_storage, secret_payload_needs_migration,
};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use tokio::sync::oneshot;
use tokio::sync::Mutex;

/// OpenAI's public OAuth client id for the Codex CLI. This is not a
/// secret — it identifies the application to the IdP and is paired with
/// PKCE for security. Same constant used by `openai/codex` and Code Buddy.
const CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";

/// OAuth issuer — the URL the user's browser is sent to and the URL we
/// POST the token exchange against.
const ISSUER: &str = "https://auth.openai.com";

/// Local port the callback server listens on. Must be in OpenAI's
/// allow-list of registered redirect URIs for the client id above.
const CALLBACK_PORT: u16 = 1455;
const FALLBACK_CALLBACK_PORT: u16 = 1457;
const CODEX_ORIGINATOR: &str = "codex_cli_rs";

/// OAuth scopes requested at sign-in. `offline_access` is what gives us a
/// refresh_token so we don't have to re-prompt the user every hour. The
/// `api.connectors.*` scopes are required by the ChatGPT backend; harmless
/// to request even when we only intend to call the classic `/v1` API.
const SCOPES: &str =
    "openid profile email offline_access api.connectors.read api.connectors.invoke";

/// How long to consider a cached access_token valid before refreshing it.
/// The IdP issues tokens with a TTL longer than this, but we play safe.
const TOKEN_REFRESH_AGE: Duration = Duration::from_secs(60 * 60);

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("OAuth flow failed: {0}")]
    Flow(String),
    #[error("token refresh failed: {0}")]
    Refresh(String),
    #[error("auth file IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("auth file JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("secret storage error: {0}")]
    SecretStore(#[from] code_explorer_core::secret_store::SecretStoreError),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

/// Token bundle returned by the IdP. Mirrors the JSON the
/// `https://auth.openai.com/oauth/token` endpoint returns.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct OauthTokens {
    pub id_token: String,
    pub access_token: String,
    pub refresh_token: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub account_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct OauthTokenRefresh {
    #[serde(default)]
    pub id_token: Option<String>,
    #[serde(default)]
    pub access_token: Option<String>,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub account_id: Option<String>,
}

/// Authentication material required by ChatGPT's Codex Responses backend.
///
/// The access token alone is not quite enough for parity with OpenAI Codex:
/// Codex also forwards the ChatGPT account id and FedRAMP marker when present.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatGptAuth {
    pub access_token: String,
    pub account_id: Option<String>,
    pub email: Option<String>,
    pub plan_type: Option<String>,
    pub is_fedramp: bool,
}

impl ChatGptAuth {
    fn from_tokens(tokens: &OauthTokens) -> Self {
        let claims = decode_id_token_claims(&tokens.id_token);
        let profile_claims = claims
            .as_ref()
            .and_then(|v| v.get("https://api.openai.com/profile"));
        let auth_claims = claims
            .as_ref()
            .and_then(|v| v.get("https://api.openai.com/auth"));

        let account_id = tokens
            .account_id
            .clone()
            .or_else(|| string_claim(auth_claims, "chatgpt_account_id"));
        let email = string_claim(profile_claims, "email")
            .or_else(|| claims.as_ref().and_then(|v| string_claim(Some(v), "email")));
        let plan_type = string_claim(auth_claims, "chatgpt_plan_type");
        let is_fedramp = bool_claim(auth_claims, "chatgpt_account_is_fedramp").unwrap_or(false);

        Self {
            access_token: tokens.access_token.clone(),
            account_id,
            email,
            plan_type,
            is_fedramp,
        }
    }
}

/// Persistent auth file. We store the last refresh timestamp as an ISO-8601
/// string (rather than `chrono::DateTime<Utc>`) so we don't have to enable
/// the optional `serde` feature on the workspace `chrono` dependency just
/// for this struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct AuthFile {
    pub tokens: OauthTokens,
    pub last_refresh: String,
}

fn auth_file_path() -> PathBuf {
    code_explorer_core::storage::repo_manager::get_global_dir()
        .join("auth")
        .join("openai.json")
}

fn load_auth() -> Result<Option<AuthFile>, AuthError> {
    let path = auth_file_path();
    match std::fs::read(&path) {
        Ok(stored) => {
            let needs_migration = secret_payload_needs_migration(&stored);
            let raw = decode_secret_from_storage(&stored)?;
            let parsed: AuthFile = serde_json::from_slice(&raw)?;
            if needs_migration {
                save_auth(&parsed)?;
            }
            Ok(Some(parsed))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(AuthError::Io(e)),
    }
}

fn save_auth(auth: &AuthFile) -> Result<(), AuthError> {
    let path = auth_file_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let raw = serde_json::to_string_pretty(auth)?;
    let stored = encode_secret_for_storage(raw.as_bytes())?;
    let mut options = OpenOptions::new();
    options.create(true).truncate(true).write(true);
    #[cfg(unix)]
    options.mode(0o600);
    let mut file = options.open(&path)?;
    file.write_all(&stored)?;
    restrict_auth_file_permissions(&path)?;
    Ok(())
}

#[cfg(unix)]
fn restrict_auth_file_permissions(path: &Path) -> Result<(), AuthError> {
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    Ok(())
}

#[cfg(windows)]
fn restrict_auth_file_permissions(path: &Path) -> Result<(), AuthError> {
    let Some(username) = std::env::var("USERNAME")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
    else {
        tracing::warn!(
            "Could not restrict ChatGPT auth file ACL: USERNAME environment variable is missing"
        );
        return Ok(());
    };

    let principal = std::env::var("USERDOMAIN")
        .ok()
        .map(|domain| domain.trim().to_string())
        .filter(|domain| !domain.is_empty())
        .map(|domain| format!("{domain}\\{username}"))
        .unwrap_or(username);
    let grant = format!("{principal}:F");

    match std::process::Command::new("icacls")
        .arg(path)
        .args(["/inheritance:r", "/grant:r"])
        .arg(&grant)
        .output()
    {
        Ok(output) if output.status.success() => {}
        Ok(output) => {
            tracing::warn!(
                "Could not restrict ChatGPT auth file ACL with icacls (status={}): {}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            );
        }
        Err(err) => {
            tracing::warn!("Could not run icacls for ChatGPT auth file ACL: {err}");
        }
    }
    Ok(())
}

#[cfg(not(any(unix, windows)))]
fn restrict_auth_file_permissions(_path: &Path) -> Result<(), AuthError> {
    Ok(())
}

/// Remove the cached tokens. `code-explorer logout` calls this.
pub fn clear() -> Result<(), AuthError> {
    let path = auth_file_path();
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(AuthError::Io(e)),
    }
}

#[derive(Debug, Clone)]
struct PkceCodes {
    code_verifier: String,
    code_challenge: String,
}

/// Generate a fresh PKCE pair. Identical to `openai/codex/codex-rs/login/pkce.rs`.
fn generate_pkce() -> PkceCodes {
    let mut bytes = [0u8; 64];
    rand::rng().fill_bytes(&mut bytes);
    let code_verifier = URL_SAFE_NO_PAD.encode(bytes);
    let digest = Sha256::digest(code_verifier.as_bytes());
    let code_challenge = URL_SAFE_NO_PAD.encode(digest);
    PkceCodes {
        code_verifier,
        code_challenge,
    }
}

fn random_state() -> String {
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

fn callback_redirect_uri(port: u16) -> String {
    format!("http://localhost:{port}/auth/callback")
}

async fn bind_callback_listener() -> Result<(tokio::net::TcpListener, u16), AuthError> {
    let ports = [CALLBACK_PORT, FALLBACK_CALLBACK_PORT];
    let mut last_error = None;

    for port in ports {
        let addr: SocketAddr = ([127, 0, 0, 1], port).into();
        match tokio::net::TcpListener::bind(addr).await {
            Ok(listener) => return Ok((listener, port)),
            Err(err) if err.kind() == std::io::ErrorKind::AddrInUse => {
                tracing::warn!("OAuth callback port {port} is already in use; trying fallback");
                last_error = Some(err);
            }
            Err(err) => {
                return Err(AuthError::Flow(format!(
                    "bind callback server on {addr}: {err}"
                )));
            }
        }
    }

    Err(AuthError::Flow(format!(
        "callback ports {CALLBACK_PORT} and {FALLBACK_CALLBACK_PORT} are both unavailable: {}",
        last_error
            .map(|e| e.to_string())
            .unwrap_or_else(|| "unknown bind error".to_string())
    )))
}

/// Exchange the authorization code received from the IdP for the bearer
/// token bundle. Posts an `application/x-www-form-urlencoded` body — the
/// IdP rejects JSON bodies on this endpoint.
async fn exchange_code(
    client: &reqwest::Client,
    code: &str,
    code_verifier: &str,
    redirect_uri: &str,
) -> Result<OauthTokens, AuthError> {
    let form = [
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", redirect_uri),
        ("client_id", CLIENT_ID),
        ("code_verifier", code_verifier),
    ];
    let resp = client
        .post(format!("{ISSUER}/oauth/token"))
        .form(&form)
        .send()
        .await
        .map_err(|e| AuthError::Flow(format!("token exchange request: {e}")))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        let body = sanitize_llm_error_body(&body, &[code, code_verifier], 300);
        return Err(AuthError::Flow(format!(
            "token exchange returned {status}: {body}"
        )));
    }
    let tokens: OauthTokens = resp
        .json()
        .await
        .map_err(|e| AuthError::Flow(format!("token exchange JSON: {e}")))?;
    Ok(tokens)
}

/// Use the refresh_token to get a fresh access_token. The IdP rotates the
/// refresh_token on every call, so we always overwrite both.
async fn refresh_tokens(
    client: &reqwest::Client,
    refresh_token: &str,
) -> Result<OauthTokenRefresh, AuthError> {
    let body = json!({
        "grant_type": "refresh_token",
        "refresh_token": refresh_token,
        "client_id": CLIENT_ID,
    });
    let resp = client
        .post(format!("{ISSUER}/oauth/token"))
        .json(&body)
        .send()
        .await
        .map_err(|e| AuthError::Refresh(e.to_string()))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        let body = sanitize_llm_error_body(&body, &[refresh_token], 300);
        return Err(AuthError::Refresh(format!("{status}: {body}")));
    }
    let tokens: OauthTokenRefresh = resp
        .json()
        .await
        .map_err(|e| AuthError::Refresh(format!("JSON: {e}")))?;
    Ok(tokens)
}

fn apply_token_refresh(tokens: &mut OauthTokens, fresh: OauthTokenRefresh) {
    let OauthTokenRefresh {
        id_token,
        access_token,
        refresh_token,
        account_id,
    } = fresh;

    if let Some(id_token) = id_token {
        tokens.id_token = id_token;
    }
    if let Some(access_token) = access_token {
        tokens.access_token = access_token;
    }
    if let Some(refresh_token) = refresh_token {
        tokens.refresh_token = refresh_token;
    }
    if account_id.is_some() {
        tokens.account_id = account_id;
    }
}

async fn load_current_auth() -> Result<Option<AuthFile>, AuthError> {
    let Some(mut auth) = load_auth()? else {
        return Ok(None);
    };
    let last_refresh: chrono::DateTime<Utc> = auth
        .last_refresh
        .parse::<chrono::DateTime<chrono::FixedOffset>>()
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now() - chrono::Duration::days(1));
    let age = (Utc::now() - last_refresh)
        .to_std()
        .unwrap_or(Duration::ZERO);
    if age < TOKEN_REFRESH_AGE {
        return Ok(Some(auth));
    }
    // Stale — refresh.
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .context("build reqwest client")?;
    match refresh_tokens(&client, &auth.tokens.refresh_token).await {
        Ok(fresh) => {
            apply_token_refresh(&mut auth.tokens, fresh);
            auth.last_refresh = Utc::now().to_rfc3339();
            save_auth(&auth)?;
            Ok(Some(auth))
        }
        Err(AuthError::Refresh(msg)) => {
            // Refresh failed: likely revoked or expired. Wipe credentials so
            // the next call falls back cleanly to `chat-config.json::api_key`.
            tracing::warn!("OAuth refresh failed ({msg}); clearing cached tokens");
            let _ = clear();
            Ok(None)
        }
        Err(other) => Err(other),
    }
}

/// Returns usable ChatGPT/Codex backend auth, opportunistically refreshing the
/// cached access token when it's older than [`TOKEN_REFRESH_AGE`].
///
/// Returns `Ok(None)` when no credentials are on disk yet, so callers can ask
/// the user to run `code-explorer login`.
pub async fn get_chatgpt_auth() -> Result<Option<ChatGptAuth>, AuthError> {
    Ok(load_current_auth()
        .await?
        .map(|auth| ChatGptAuth::from_tokens(&auth.tokens)))
}

/// Returns only the access token for older call sites. New ChatGPT backend
/// integrations should use [`get_chatgpt_auth`] so they can forward the
/// account headers expected by the Codex backend.
#[allow(dead_code)]
pub async fn get_access_token() -> Result<Option<String>, AuthError> {
    Ok(get_chatgpt_auth().await?.map(|auth| auth.access_token))
}

fn decode_id_token_claims(id_token: &str) -> Option<serde_json::Value> {
    let payload = id_token.split('.').nth(1)?;
    let decoded = URL_SAFE_NO_PAD.decode(payload).ok()?;
    serde_json::from_slice(&decoded).ok()
}

fn string_claim(value: Option<&serde_json::Value>, key: &str) -> Option<String> {
    value
        .and_then(|v| v.get(key))
        .and_then(|v| v.as_str())
        .map(ToOwned::to_owned)
}

fn bool_claim(value: Option<&serde_json::Value>, key: &str) -> Option<bool> {
    value.and_then(|v| v.get(key)).and_then(|v| v.as_bool())
}

/// State the callback handler hands back to the main flow after a
/// successful exchange. `Err` carries the user-facing failure message.
type CallbackOutcome = Result<OauthTokens, String>;

#[derive(Clone)]
struct CallbackState {
    code_verifier: Arc<String>,
    redirect_uri: Arc<String>,
    expected_state: Arc<String>,
    sender: Arc<Mutex<Option<oneshot::Sender<CallbackOutcome>>>>,
    http: reqwest::Client,
}

#[derive(Debug, Deserialize)]
struct CallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
}

async fn callback_handler(
    State(st): State<CallbackState>,
    Query(q): Query<CallbackQuery>,
) -> impl IntoResponse {
    let send = |outcome: CallbackOutcome| async {
        // Take the sender exactly once — second redirect would be a no-op.
        if let Some(tx) = st.sender.lock().await.take() {
            let _ = tx.send(outcome);
        }
    };

    if let Some(err) = q.error {
        let detail = q.error_description.unwrap_or_else(|| err.clone());
        send(Err(format!("OAuth provider error: {detail}"))).await;
        return (
            StatusCode::BAD_REQUEST,
            Html(error_html("OpenAI a refusé la connexion", &detail)),
        );
    }

    let (Some(code), Some(state)) = (q.code, q.state) else {
        send(Err("callback missing code or state".to_string())).await;
        return (
            StatusCode::BAD_REQUEST,
            Html(error_html(
                "Réponse incomplète",
                "code ou state manquant dans le callback",
            )),
        );
    };

    if state != *st.expected_state {
        send(Err("state mismatch (possible CSRF)".to_string())).await;
        return (
            StatusCode::BAD_REQUEST,
            Html(error_html(
                "État invalide",
                "Le state OAuth ne correspond pas — possible attaque CSRF.",
            )),
        );
    }

    match exchange_code(&st.http, &code, &st.code_verifier, &st.redirect_uri).await {
        Ok(tokens) => {
            send(Ok(tokens)).await;
            (StatusCode::OK, Html(success_html()))
        }
        Err(e) => {
            send(Err(e.to_string())).await;
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Html(error_html("Échec de l'échange de jetons", &e.to_string())),
            )
        }
    }
}

fn success_html() -> String {
    r##"<!doctype html>
<html lang="fr"><head><meta charset="utf-8"><title>Code Explorer — Connecté</title>
<style>body{font-family:system-ui;background:#0a0a0a;color:#e5e5e5;display:flex;align-items:center;justify-content:center;min-height:100vh;margin:0}
.card{background:#171717;border:1px solid #262626;border-radius:12px;padding:32px 40px;max-width:420px;text-align:center}
h1{font-size:18px;margin:0 0 8px;color:#a7f3d0}
p{margin:0;font-size:14px;color:#a3a3a3;line-height:1.5}</style>
</head><body><div class="card"><h1>✅ Authentifié à ChatGPT</h1>
<p>Tu peux fermer cet onglet et retourner dans ton terminal — Code Explorer a stocké ton jeton.</p>
<script>setTimeout(() => window.close(), 1200)</script>
</div></body></html>"##
        .to_string()
}

fn error_html(title: &str, detail: &str) -> String {
    let safe_title = html_escape(title);
    let safe_detail = html_escape(detail);
    format!(
        r##"<!doctype html>
<html lang="fr"><head><meta charset="utf-8"><title>Code Explorer — Erreur OAuth</title>
<style>body{{font-family:system-ui;background:#0a0a0a;color:#e5e5e5;display:flex;align-items:center;justify-content:center;min-height:100vh;margin:0}}
.card{{background:#171717;border:1px solid #7f1d1d;border-radius:12px;padding:32px 40px;max-width:520px}}
h1{{font-size:18px;margin:0 0 12px;color:#fca5a5}}
pre{{margin:0;font-size:12px;color:#a3a3a3;white-space:pre-wrap;word-break:break-word}}</style>
</head><body><div class="card"><h1>{safe_title}</h1><pre>{safe_detail}</pre></div></body></html>"##
    )
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Run the full interactive login flow: spin a callback server, open the
/// browser, wait for the redirect, exchange the code, persist the tokens.
///
/// Blocks until the user completes (or rejects) the flow in their browser.
/// 5-minute timeout so a forgotten browser tab doesn't leave a zombie task.
pub async fn login() -> Result<(), AuthError> {
    let pkce = generate_pkce();
    let state = random_state();
    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .context("build reqwest client")?;

    let (tx, rx) = oneshot::channel::<CallbackOutcome>();
    let (listener, actual_port) = bind_callback_listener().await?;
    let redirect_uri = callback_redirect_uri(actual_port);
    let cb_state = CallbackState {
        code_verifier: Arc::new(pkce.code_verifier.clone()),
        redirect_uri: Arc::new(redirect_uri.clone()),
        expected_state: Arc::new(state.clone()),
        sender: Arc::new(Mutex::new(Some(tx))),
        http,
    };

    let app = Router::new()
        .route("/auth/callback", get(callback_handler))
        .with_state(cb_state);

    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    let server = tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async {
                let _ = shutdown_rx.await;
            })
            .await
    });

    let auth_url = build_authorize_url(&redirect_uri, &pkce.code_challenge, &state);
    eprintln!("Opening browser for ChatGPT login…");
    eprintln!("  {auth_url}");
    if webbrowser::open(&auth_url).is_err() {
        eprintln!(
            "Couldn't auto-open the browser — copy the URL above into your browser manually."
        );
    }

    // Wait for callback (or timeout).
    let outcome = match tokio::time::timeout(Duration::from_secs(300), rx).await {
        Ok(Ok(outcome)) => outcome,
        Ok(Err(_)) => Err("callback channel dropped before completion".to_string()),
        Err(_) => Err("login timed out after 5 minutes".to_string()),
    };

    let _ = shutdown_tx.send(());
    let _ = server.await;

    let tokens = outcome.map_err(AuthError::Flow)?;
    let auth = AuthFile {
        tokens,
        last_refresh: Utc::now().to_rfc3339(),
    };
    save_auth(&auth)?;
    eprintln!("Saved tokens to {}", auth_file_path().display());
    Ok(())
}

fn build_authorize_url(redirect_uri: &str, code_challenge: &str, state: &str) -> String {
    let params = [
        ("response_type", "code"),
        ("client_id", CLIENT_ID),
        ("redirect_uri", redirect_uri),
        ("scope", SCOPES),
        ("code_challenge", code_challenge),
        ("code_challenge_method", "S256"),
        ("id_token_add_organizations", "true"),
        ("codex_cli_simplified_flow", "true"),
        ("state", state),
        ("originator", CODEX_ORIGINATOR),
    ];
    let qs = params
        .iter()
        .map(|(k, v)| format!("{k}={}", percent_encode(v)))
        .collect::<Vec<_>>()
        .join("&");
    format!("{ISSUER}/oauth/authorize?{qs}")
}

/// Minimal RFC 3986 percent-encoder for the URL params we emit. We escape
/// every byte that is not in the unreserved set, which is pessimistic but
/// always safe.
fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*b as char);
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unsigned_jwt(claims: serde_json::Value) -> String {
        let header = URL_SAFE_NO_PAD.encode(r#"{"alg":"none"}"#);
        let payload = URL_SAFE_NO_PAD.encode(claims.to_string());
        format!("{header}.{payload}.")
    }

    #[test]
    fn extracts_chatgpt_account_claims_from_id_token() {
        let tokens = OauthTokens {
            id_token: unsigned_jwt(json!({
                "https://api.openai.com/profile": {
                    "email": "patrice@example.com"
                },
                "https://api.openai.com/auth": {
                    "chatgpt_account_id": "acct_123",
                    "chatgpt_plan_type": "plus",
                    "chatgpt_account_is_fedramp": true
                }
            })),
            access_token: "access-token".to_string(),
            refresh_token: "refresh-token".to_string(),
            account_id: None,
        };

        let auth = ChatGptAuth::from_tokens(&tokens);

        assert_eq!(auth.access_token, "access-token");
        assert_eq!(auth.account_id.as_deref(), Some("acct_123"));
        assert_eq!(auth.email.as_deref(), Some("patrice@example.com"));
        assert_eq!(auth.plan_type.as_deref(), Some("plus"));
        assert!(auth.is_fedramp);
    }

    #[test]
    fn token_account_id_overrides_id_token_claim() {
        let tokens = OauthTokens {
            id_token: unsigned_jwt(json!({
                "https://api.openai.com/auth": {
                    "chatgpt_account_id": "acct_from_claim"
                }
            })),
            access_token: "access-token".to_string(),
            refresh_token: "refresh-token".to_string(),
            account_id: Some("acct_from_token".to_string()),
        };

        let auth = ChatGptAuth::from_tokens(&tokens);

        assert_eq!(auth.account_id.as_deref(), Some("acct_from_token"));
    }

    #[test]
    fn authorize_url_matches_codex_login_contract() {
        let auth_url = build_authorize_url(
            "http://localhost:1457/auth/callback",
            "challenge-value",
            "state-value",
        );

        assert!(auth_url.starts_with("https://auth.openai.com/oauth/authorize?"));
        assert!(auth_url.contains("redirect_uri=http%3A%2F%2Flocalhost%3A1457%2Fauth%2Fcallback"));
        assert!(auth_url.contains("originator=codex_cli_rs"));
        assert!(auth_url.contains("codex_cli_simplified_flow=true"));
        assert!(auth_url.contains("id_token_add_organizations=true"));
        assert!(auth_url.contains("code_challenge=challenge-value"));
        assert!(auth_url.contains("state=state-value"));
    }

    #[test]
    fn callback_redirect_uri_uses_actual_bound_port() {
        assert_eq!(
            callback_redirect_uri(1457),
            "http://localhost:1457/auth/callback"
        );
    }

    #[test]
    fn random_state_uses_32_bytes_of_entropy() {
        assert_eq!(random_state().len(), 43);
    }

    #[test]
    fn token_refresh_keeps_cached_values_when_fields_are_absent() {
        let mut tokens = OauthTokens {
            id_token: "old-id".to_string(),
            access_token: "old-access".to_string(),
            refresh_token: "old-refresh".to_string(),
            account_id: Some("old-account".to_string()),
        };

        apply_token_refresh(
            &mut tokens,
            OauthTokenRefresh {
                id_token: None,
                access_token: Some("new-access".to_string()),
                refresh_token: None,
                account_id: None,
            },
        );

        assert_eq!(tokens.id_token, "old-id");
        assert_eq!(tokens.access_token, "new-access");
        assert_eq!(tokens.refresh_token, "old-refresh");
        assert_eq!(tokens.account_id.as_deref(), Some("old-account"));
    }
}

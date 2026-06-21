//! OAuth authentication for code-explorer.
//!
//! This crate-level entry point currently exposes the **Codex** OAuth flow,
//! which lets the user authenticate against their ChatGPT Pro / Plus
//! subscription and use that quota through ChatGPT's Codex Responses backend.
//!
//! Why we need this: the LLM tool loop in `commands::ask::ask_question_with_tools`
//! can fire 5–8 LLM calls per question (one per tool round-trip). ChatGPT
//! subscription auth must stay scoped to the `provider = "chatgpt"` backend;
//! other OpenAI-compatible providers keep using their configured API key.
//!
//! Borrowed from the `openai/codex` Rust CLI, which OpenAI publishes under
//! Apache-2.0. Their crate ships the official PKCE-based Authorization
//! Code Flow that the ChatGPT IdP accepts. Our port keeps only the parts
//! that work standalone (no `codex-config`, no `codex-utils-template`,
//! no `tiny_http` dependency — we use `axum` which is already in the tree).

pub mod codex_oauth;

pub use codex_oauth::{clear, get_chatgpt_auth, login, ChatGptAuth};

//! The `config` command: validate LLM configuration.

use anyhow::Result;
use colored::Colorize;
use code_explorer_core::llm::{sanitize_llm_error_body, PROMPT_CONTEXT_SAFETY};

/// Format an API key as `prefix...suffix` for display, hiding the middle.
///
/// Operates on chars, not bytes. The previous inline implementation
/// (`&api_key[..api_key.len().min(8)]` + `&api_key[api_key.len() - 4..]`)
/// panicked whenever a byte boundary fell inside a multi-byte UTF-8
/// character — e.g., the 11-byte string `"abcéééé"` has char boundaries
/// at 0,1,2,3,5,7,9,11, so `[..8]` panics with
/// `"byte index 8 is not a char boundary; it is inside 'é' (bytes 7..9)"`.
/// Real-world API keys are usually ASCII, but a diagnostic helper that
/// crashes on user-supplied non-ASCII input is its own bug.
fn mask_api_key(api_key: &str) -> String {
    let char_count = api_key.chars().count();
    let prefix: String = api_key.chars().take(8).collect();
    if char_count > 8 {
        let suffix: String = api_key.chars().skip(char_count.saturating_sub(4)).collect();
        format!("{}...{}", prefix, suffix)
    } else {
        format!("{}...", prefix)
    }
}

fn sanitize_error_preview(body: &str, api_key: &str) -> String {
    sanitize_llm_error_body(body, &[api_key], 200)
}

pub fn run_test() -> Result<()> {
    let config = super::generate::load_llm_config();
    let config = match config {
        Some(c) => c,
        None => {
            println!("{} No LLM config found.", "ERROR".red());
            println!();
            println!("  Create ~/.codeexplorer/chat-config.json with:");
            println!();
            println!("  {{");
            println!("    \"provider\": \"gemini\",");
            println!("    \"api_key\": \"YOUR_API_KEY\",");
            println!(
                "    \"base_url\": \"https://generativelanguage.googleapis.com/v1beta/openai\","
            );
            println!("    \"model\": \"gemini-2.5-flash\",");
            println!("    \"max_tokens\": 8192,");
            println!("    \"reasoning_effort\": \"high\"");
            println!("  }}");
            return Ok(());
        }
    };

    println!("{} Config loaded:", "OK".green());
    println!("  Provider:  {}", config.provider);
    println!("  Model:     {}", config.model);
    println!("  Base URL:  {}", config.base_url);
    println!("  Max tokens: {}", config.max_tokens);
    println!("  API key:   {}", mask_api_key(&config.api_key));

    if config.provider.eq_ignore_ascii_case("chatgpt") {
        println!();
        println!("{} Checking ChatGPT OAuth login...", "->".cyan());
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        match runtime.block_on(crate::auth::get_chatgpt_auth()) {
            Ok(Some(auth)) => {
                println!("{}", "OK ChatGPT login found.".green());
                if let Some(email) = auth.email.as_deref() {
                    println!("  Account: {}", email);
                }
                if let Some(plan) = auth.plan_type.as_deref() {
                    println!("  Plan:    {}", plan);
                }
            }
            Ok(None) => {
                println!("{}", "ERROR No ChatGPT login found.".red());
                println!("  Run `code-explorer login`, then retry `code-explorer config test`.");
            }
            Err(e) => {
                println!("{} ChatGPT auth check failed: {}", "ERROR".red(), e);
            }
        }
        return Ok(());
    }

    // Test connectivity
    println!();
    println!("{} Testing connection...", "->".cyan());

    let url = format!("{}/chat/completions", config.base_url.trim_end_matches('/'));
    let body = serde_json::json!({
        "model": config.model,
        "messages": [
            {"role": "system", "content": PROMPT_CONTEXT_SAFETY},
            {"role": "user", "content": "Say hello in one word."}
        ],
        "max_tokens": 10,
        "temperature": 0.0,
    });

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()?;

    let mut request = client.post(&url).json(&body);
    if !config.api_key.is_empty() {
        request = request.header("Authorization", format!("Bearer {}", config.api_key));
    }

    match request.send() {
        Ok(resp) => {
            let status = resp.status();
            if status.is_success() {
                println!("{} Connection successful (HTTP {})", "OK".green(), status);
                if let Ok(json) = resp.json::<serde_json::Value>() {
                    if let Some(content) = json
                        .get("choices")
                        .and_then(|c| c.get(0))
                        .and_then(|c| c.get("message"))
                        .and_then(|m| m.get("content"))
                        .and_then(|v| v.as_str())
                    {
                        println!("  Response: {}", content.trim());
                    }
                }
            } else {
                println!(
                    "{} HTTP {} — {}",
                    "ERROR".red(),
                    status,
                    status.canonical_reason().unwrap_or("")
                );
                if let Ok(body) = resp.text() {
                    println!("  {}", sanitize_error_preview(&body, &config.api_key));
                }
            }
        }
        Err(e) => {
            println!("{} Connection failed: {}", "ERROR".red(), e);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mask_api_key_short_ascii() {
        // <= 8 chars: no suffix
        assert_eq!(mask_api_key(""), "...");
        assert_eq!(mask_api_key("abc"), "abc...");
        assert_eq!(mask_api_key("12345678"), "12345678...");
    }

    #[test]
    fn mask_api_key_long_ascii() {
        // > 8 chars: prefix(8)...suffix(4)
        assert_eq!(mask_api_key("sk-1234567890abcdef"), "sk-12345...cdef");
    }

    #[test]
    fn mask_api_key_unicode_does_not_panic() {
        // The pre-fix code panicked here:
        //   `byte index 8 is not a char boundary; it is inside 'é'`.
        // 11 bytes / 7 chars: short branch (no suffix), but the OLD code
        // unconditionally evaluated `&api_key[..len.min(8)]` which used 8 as
        // a byte index and crashed.
        let _ = mask_api_key("abcéééé");

        // 18 bytes / 9 chars: long branch — exercises both the take(8) and
        // the suffix extraction.
        let result = mask_api_key("aaaa🌍bbbcccc");
        // Just assert it returned something containing the prefix + ellipsis.
        assert!(result.contains("..."));

        // Pure non-ASCII: every char is multi-byte. Must not panic.
        let _ = mask_api_key("日本語秘密キーずっと長い");
    }

    #[test]
    fn mask_api_key_exact_eight_chars() {
        // Exactly 8 chars goes to the no-suffix branch (count > 8 is false).
        assert_eq!(mask_api_key("abcdefgh"), "abcdefgh...");
    }

    #[test]
    fn sanitize_error_preview_redacts_api_key() {
        let preview = sanitize_error_preview("provider echoed sk-live-secret", "sk-live-secret");

        assert!(!preview.contains("sk-live-secret"));
        assert!(preview.contains("[redacted-secret]"));
    }
}

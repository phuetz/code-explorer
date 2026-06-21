//! Responses API backend (chatgpt.com/backend-api/codex/responses) for tool-loop.
//!
//! This module handles the Responses API format used by ChatGPT's backend,
//! distinct from the classic OpenAI chat/completions format. The key differences:
//! - Request body uses `input` (array of items) instead of `messages`
//! - System prompt goes into `instructions` field
//! - Responses come as SSE events with a different type taxonomy
//! - Tool calls are `{ type: "function_call", name: ..., arguments: ..., call_id: ... }`
//! - Tool results are `{ type: "function_call_output", call_id: ..., output: ... }`
//!
//! References: codex-rs source (github.com/openai/codex)

use anyhow::{anyhow, Result};
use code_explorer_core::llm::sanitize_llm_error_body;
use reqwest::{header, Client, RequestBuilder};
use serde_json::{json, Value};

use super::StreamEvent;
use crate::auth::ChatGptAuth;

const CHATGPT_RESPONSES_URL: &str = "https://chatgpt.com/backend-api/codex/responses";
const ORIGINATOR: &str = "codex_cli_rs";

#[derive(Debug, Clone)]
pub struct ToolCallRequest {
    pub id: String, // call_id in Responses API
    pub name: String,
    pub args: String, // JSON arguments as string
}

pub struct ResponsesModelConfig<'a> {
    pub model: &'a str,
    pub reasoning_effort: &'a str,
}

/// Call the Responses API for one turn, handle streaming SSE, and return text + tool calls.
///
/// Accumulates `output_text.delta` events into the response text.
/// Collects function calls from `output_item.done` events where `item.type == "function_call"`.
/// Modifies `input` in-place to append the assistant's function_call items.
pub async fn call_responses_turn(
    client: &Client,
    auth: &ChatGptAuth,
    model_config: ResponsesModelConfig<'_>,
    instructions: &str,
    input: &mut Vec<Value>,
    tools: &[Value],
    stream_cb: Option<&(dyn Fn(StreamEvent) + Send + Sync)>,
) -> Result<(String, Vec<ToolCallRequest>)> {
    let responses_tools = to_responses_tools(tools);
    // Build request body.
    let mut body = json!({
        "model": model_config.model,
        "instructions": instructions,
        "input": input,
        "store": false,
        "stream": true,
    });
    if !responses_tools.is_empty() {
        body["tools"] = json!(responses_tools);
        body["tool_choice"] = json!("auto");
        body["parallel_tool_calls"] = json!(true);
    }
    if let Some(effort) = responses_reasoning_effort(model_config.reasoning_effort) {
        body["reasoning"] = json!({ "effort": effort });
    }

    // Make request with Bearer auth and SSE headers.
    let request = client
        .post(CHATGPT_RESPONSES_URL)
        .bearer_auth(&auth.access_token)
        .header("Accept", "text/event-stream")
        .header("Content-Type", "application/json")
        .header("originator", ORIGINATOR)
        .header(
            header::USER_AGENT,
            concat!("code-explorer-cli/", env!("CARGO_PKG_VERSION")),
        )
        .json(&body);
    let response = apply_chatgpt_account_headers(request, auth).send().await?;

    if !response.status().is_success() {
        let status = response.status();
        let body_text = response.text().await.unwrap_or_default();
        return Err(anyhow!(
            "Responses API error: {} {}",
            status,
            sanitize_responses_error_body(&body_text, auth)
        ));
    }

    // Stream the SSE response and collect events.
    let mut full_text = String::new();
    let mut tool_calls = Vec::new();
    let mut assistant_items = Vec::new(); // Append to input after parsing.

    let body_text = response.text().await?;
    for line in body_text.lines() {
        if !line.starts_with("data: ") {
            continue;
        }

        let json_str = &line[6..]; // Strip "data: " prefix.
        let event: Value = match serde_json::from_str(json_str) {
            Ok(v) => v,
            Err(_) => continue, // Malformed JSON, skip.
        };

        let event_type = event["type"].as_str().unwrap_or("");

        match event_type {
            "response.output_text.delta" => {
                if let Some(delta) = event["delta"].as_str() {
                    full_text.push_str(delta);
                    if let Some(cb) = stream_cb {
                        cb(StreamEvent::Delta(delta.to_string()));
                    }
                }
            }

            "response.output_text.done"
                if full_text.is_empty() => {
                    if let Some(text) = event["text"].as_str().filter(|text| !text.is_empty()) {
                        full_text.push_str(text);
                        if let Some(cb) = stream_cb {
                            cb(StreamEvent::Delta(text.to_string()));
                        }
                    }
                }

            "response.output_item.done" => {
                if let Some(item) = event["item"].as_object() {
                    if let Some(item_type) = item.get("type").and_then(|v| v.as_str()) {
                        if item_type == "function_call" {
                            // Extract function call details.
                            if let (Some(name), Some(call_id), Some(args)) = (
                                item.get("name").and_then(|v| v.as_str()),
                                item.get("call_id").and_then(|v| v.as_str()),
                                item.get("arguments").and_then(|v| v.as_str()),
                            ) {
                                tool_calls.push(ToolCallRequest {
                                    id: call_id.to_string(),
                                    name: name.to_string(),
                                    args: args.to_string(),
                                });

                                // Also collect the function_call item itself for input history.
                                assistant_items.push(json!({
                                    "type": "function_call",
                                    "name": name,
                                    "arguments": args,
                                    "call_id": call_id,
                                }));
                            }
                        } else if full_text.is_empty()
                            && matches!(item_type, "message" | "assistant_message")
                        {
                            let item_value = Value::Object(item.clone());
                            let text = extract_output_item_text(&item_value);
                            if !text.is_empty() {
                                full_text.push_str(&text);
                                if let Some(cb) = stream_cb {
                                    cb(StreamEvent::Delta(text));
                                }
                            }
                        }
                    }
                }
            }

            "response.completed" => {
                // End of turn. Check for response_id if needed (not used here).
            }

            "response.failed" => {
                if let Some(error) = event["response"]["error"].as_object() {
                    let code = error
                        .get("code")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown");
                    let message = error
                        .get("message")
                        .and_then(|v| v.as_str())
                        .unwrap_or("no message");
                    return Err(anyhow!("Responses API failed ({}): {}", code, message));
                }
            }

            _ => {
                // Ignore other event types (metadata, reasoning, etc.).
            }
        }
    }

    // Append function_call items to input for the next turn's context.
    for item in assistant_items {
        input.push(item);
    }

    Ok((full_text, tool_calls))
}

fn to_responses_tools(tools: &[Value]) -> Vec<Value> {
    tools
        .iter()
        .map(|tool| {
            let Some(function) = tool.get("function") else {
                return tool.clone();
            };
            json!({
                "type": "function",
                "name": function.get("name").cloned().unwrap_or(Value::Null),
                "description": function.get("description").cloned().unwrap_or(Value::Null),
                "parameters": function.get("parameters").cloned().unwrap_or_else(|| json!({})),
            })
        })
        .collect()
}

fn responses_reasoning_effort(reasoning_effort: &str) -> Option<String> {
    let effort = reasoning_effort.trim().to_ascii_lowercase();
    match effort.as_str() {
        "" => None,
        "none" | "minimal" | "low" | "medium" | "high" | "xhigh" => Some(effort),
        _ => None,
    }
}

fn apply_chatgpt_account_headers(
    mut request: RequestBuilder,
    auth: &ChatGptAuth,
) -> RequestBuilder {
    if let Some(account_id) = auth.account_id.as_deref() {
        request = request.header("ChatGPT-Account-ID", account_id);
    }
    if auth.is_fedramp {
        request = request.header("X-OpenAI-Fedramp", "true");
    }
    request
}

fn sanitize_responses_error_body(body: &str, auth: &ChatGptAuth) -> String {
    const MAX_ERROR_BODY_CHARS: usize = 1_200;
    sanitize_llm_error_body(body, &[&auth.access_token], MAX_ERROR_BODY_CHARS)
}

/// Append tool results to the input array in Responses API format.
pub fn append_tool_result(input: &mut Vec<Value>, call_id: &str, output: &str) {
    input.push(json!({
        "type": "function_call_output",
        "call_id": call_id,
        "output": output,
    }));
}

fn extract_output_item_text(item: &Value) -> String {
    if let Some(text) = item.get("text").and_then(Value::as_str) {
        return text.to_string();
    }
    if let Some(text) = item.get("output_text").and_then(Value::as_str) {
        return text.to_string();
    }
    if let Some(text) = item.get("content").and_then(Value::as_str) {
        return text.to_string();
    }
    item.get("content")
        .and_then(Value::as_array)
        .map(|parts| {
            parts
                .iter()
                .filter_map(|part| {
                    part.get("text")
                        .and_then(Value::as_str)
                        .or_else(|| part.get("content").and_then(Value::as_str))
                })
                .collect::<Vec<_>>()
                .join("")
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::{
        extract_output_item_text, responses_reasoning_effort, sanitize_responses_error_body,
        to_responses_tools, ORIGINATOR,
    };
    use crate::auth::ChatGptAuth;
    use serde_json::json;

    #[test]
    fn responses_originator_matches_official_codex_client() {
        assert_eq!(ORIGINATOR, "codex_cli_rs");
    }

    #[test]
    fn sanitize_responses_error_body_redacts_access_token() {
        let auth = ChatGptAuth {
            access_token: "chatgpt-access-token".to_string(),
            account_id: Some("acct_123".to_string()),
            email: None,
            plan_type: None,
            is_fedramp: false,
        };

        let sanitized =
            sanitize_responses_error_body("bad bearer chatgpt-access-token in request", &auth);

        assert!(!sanitized.contains("chatgpt-access-token"));
        assert!(sanitized.contains("[redacted-secret]"));
    }

    #[test]
    fn responses_tools_are_flattened_for_codex_backend() {
        let tools = vec![json!({
            "type": "function",
            "function": {
                "name": "search_code",
                "description": "Search symbols",
                "parameters": {"type": "object"}
            }
        })];

        let converted = to_responses_tools(&tools);
        assert_eq!(converted[0]["type"], "function");
        assert_eq!(converted[0]["name"], "search_code");
        assert!(converted[0].get("function").is_none());
        assert_eq!(converted[0]["parameters"]["type"], "object");
    }

    #[test]
    fn responses_reasoning_effort_accepts_gpt55_levels() {
        assert_eq!(
            responses_reasoning_effort(" HIGH "),
            Some("high".to_string())
        );
        assert_eq!(
            responses_reasoning_effort("minimal"),
            Some("minimal".to_string())
        );
        assert_eq!(
            responses_reasoning_effort("xhigh"),
            Some("xhigh".to_string())
        );
        assert_eq!(responses_reasoning_effort(""), None);
        assert_eq!(responses_reasoning_effort("surprise"), None);
    }

    #[test]
    fn extracts_message_text_from_output_item_shapes() {
        let item = json!({
            "type": "message",
            "content": [
                {"type": "output_text", "text": "Bonjour "},
                {"type": "output_text", "text": "Code Explorer"}
            ]
        });

        assert_eq!(extract_output_item_text(&item), "Bonjour Code Explorer");

        let simple_item = json!({
            "type": "message",
            "content": "Texte direct"
        });
        assert_eq!(extract_output_item_text(&simple_item), "Texte direct");
    }
}

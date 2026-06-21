use futures_util::StreamExt;
use reqwest::Client;
use serde_json::Value;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use super::{
    sanitize_llm_error_body, LlmProvider, LlmResponseChunk, LlmStream, Message, ToolCall,
    ToolDefinition,
};

pub struct OpenAILlmProvider {
    client: Client,
    base_url: String,
    api_key: String,
    model: String,
    max_tokens: u32,
    reasoning_effort: String,
}

/// Per-model-family defaults for `(max_tokens, reasoning_effort)`.
///
/// Gemini Flash-tier models share a `max_tokens` budget with the thinking
/// tokens emitted by Gemini 2.5+. On `gemini-3-flash-preview` with
/// `reasoning_effort = high` + `max_tokens = 8192`, the entire budget can be
/// spent on thoughts, leaving `finish_reason = length` and zero visible text.
/// Google's OpenAI-compat docs recommend `reasoning_effort = low` for Flash
/// and a higher ceiling that leaves room for both thinking and output.
///
/// Pro-tier models benefit from a wider thinking budget but also need more
/// output room; we bump `max_tokens` and use "medium"/"high".
///
/// Callers can still override these via the provider constructor (the values
/// passed in win over the defaults). The match is intentionally broad so
/// sub-variants (`-preview`, `-lite-preview`, `-exp-01`, ...) pick up the
/// right family.
pub fn defaults_for(model: &str) -> (u32, &'static str) {
    let m = model.trim().to_ascii_lowercase();
    if m.starts_with("gemini-3.1-flash") || m.starts_with("gemini-3-flash") {
        (32768, "low")
    } else if m.starts_with("gemini-3.1-pro") || m.starts_with("gemini-3-pro") {
        (16384, "medium")
    } else if m.starts_with("gemini-2.5-flash") {
        (8192, "medium")
    } else if m.starts_with("gemini-2.5-pro") {
        (16384, "high")
    } else {
        (8192, "medium")
    }
}

impl OpenAILlmProvider {
    pub fn new(
        base_url: String,
        api_key: String,
        model: String,
        max_tokens: u32,
        reasoning_effort: String,
    ) -> Result<Self, String> {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

        // Apply per-model defaults when callers did not explicitly override.
        // Convention: `max_tokens == 0` or `reasoning_effort.is_empty()` means
        // "not set", so we fall back to the family default.
        let (default_max, default_effort) = defaults_for(&model);
        let max_tokens = if max_tokens == 0 {
            default_max
        } else {
            max_tokens
        };
        let reasoning_effort = if reasoning_effort.trim().is_empty() {
            default_effort.to_string()
        } else {
            reasoning_effort
        };

        Ok(Self {
            client,
            base_url,
            api_key,
            model,
            max_tokens,
            reasoning_effort,
        })
    }

    /// Returns true when we're talking to a Gemini model via the OpenAI
    /// compatibility layer. Used to gate Google-specific passthrough payloads
    /// so we don't break other providers (OpenAI, OpenRouter, LiteLLM, …).
    fn is_gemini(&self) -> bool {
        self.model.to_ascii_lowercase().starts_with("gemini-")
    }
}

impl LlmProvider for OpenAILlmProvider {
    async fn stream_completion(
        &self,
        messages: &[Message],
        tools: &[ToolDefinition],
    ) -> Result<LlmStream, String> {
        // Empty-stream retry loop: Gemini 3.x Flash occasionally returns a
        // stream that carries only `thought_signature` chunks and zero
        // visible content, ending with `finish_reason = length` because the
        // thinking tokens consumed the entire budget. `gemini-cli` treats
        // this as `InvalidStreamError` and retries up to 4 attempts with
        // exponential backoff (see packages/core/src/core/geminiChat.ts —
        // MID_STREAM_RETRY_OPTIONS). We do the same, and on each retry we
        // downgrade `reasoning_effort` to give the model more budget for
        // output tokens.
        //
        // On Gemini Flash-tier models we ALSO pre-clamp `high` to `medium`
        // on the very first attempt. This mirrors what
        // `code-explorer-cli::commands::generate::enrichment::clamp_enrichment_effort`
        // does for document enrichment: deep chain-of-thought on a Flash
        // model "just consumes thinking tokens and amplifies rate-limit
        // pressure" (quote from that function's rationale). Users keep the
        // ability to explicitly request high via config — the clamp only
        // triggers on Gemini Flash where it's known to be counter-productive.
        let start_effort = {
            let raw = self.reasoning_effort.trim().to_ascii_lowercase();
            let is_gemini_flash =
                self.is_gemini() && self.model.to_ascii_lowercase().contains("flash");
            if is_gemini_flash && raw == "high" {
                tracing::debug!(
                    "Clamping reasoning_effort high → medium for Gemini Flash model {}",
                    self.model
                );
                "medium".to_string()
            } else {
                raw
            }
        };

        let efforts_ladder = retry_efforts_for(start_effort.as_str());

        let mut last_err: Option<String> = None;
        for (attempt, effort) in efforts_ladder.iter().enumerate() {
            if attempt > 0 {
                // 1s, 2s, 4s — same cadence as gemini-cli.
                let delay_ms = 1_000u64 << (attempt - 1);
                tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
                tracing::warn!(
                    "LLM stream retry attempt {} (effort={}, model={})",
                    attempt + 1,
                    effort,
                    self.model
                );
            }
            match self.try_stream_once(messages, tools, effort).await {
                Ok(stream) => return Ok(stream),
                Err(e) => {
                    tracing::warn!("LLM stream attempt {} failed: {}", attempt + 1, e);
                    last_err = Some(e);
                }
            }
        }
        Err(last_err.unwrap_or_else(|| "LLM stream failed with no recorded error".to_string()))
    }
}

impl OpenAILlmProvider {
    /// One attempt at the stream. Drains the SSE stream synchronously to
    /// determine whether it carried any text/tool_calls; if not, returns an
    /// `Err` so `stream_completion` retries with a degraded thinking budget.
    /// On success, returns a replay stream that yields the already-drained
    /// chunks.
    async fn try_stream_once(
        &self,
        messages: &[Message],
        tools: &[ToolDefinition],
        effort: &str,
    ) -> Result<LlmStream, String> {
        let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));

        let mut body = serde_json::json!({
            "model": self.model,
            "messages": messages,
            "max_tokens": self.max_tokens,
            "temperature": 0.3,
            "stream": true,
        });

        if !tools.is_empty() {
            body["tools"] = serde_json::json!(tools);
        }

        let effort_lc = effort.trim().to_ascii_lowercase();
        let gemini = self.is_gemini();

        if gemini {
            // For Gemini via the OpenAI-compat endpoint, `reasoning_effort`
            // and `extra_body.google.thinking_config.thinking_level` overlap
            // and must NOT be sent together (per Google docs). We use
            // `extra_body` because it also lets us pin `include_thoughts`
            // (we don't want the thought summaries in the visible output).
            let thinking_level = match effort_lc.as_str() {
                "high" => "high",
                "medium" => "medium",
                _ => "low",
            };
            body["extra_body"] = serde_json::json!({
                "google": {
                    "thinking_config": {
                        "include_thoughts": false,
                        "thinking_level": thinking_level,
                    }
                }
            });
        } else if !effort_lc.is_empty() && effort_lc != "none" {
            body["reasoning_effort"] = Value::String(effort_lc);
        }

        let mut request = self.client.post(&url).json(&body);

        if !self.api_key.is_empty() {
            request = request.header("Authorization", format!("Bearer {}", self.api_key));
        }

        let response = request
            .send()
            .await
            .map_err(|e| format!("LLM API request failed: {}", e))?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(format!(
                "LLM API error ({}): {}",
                status,
                sanitize_llm_error_body(&error_text, &[&self.api_key], 300)
            ));
        }

        // Drain the SSE stream eagerly so we can detect an empty response
        // (zero text + zero tool_calls) and trigger a retry upstream. This
        // matches gemini-cli's InvalidStreamError / NO_RESPONSE_TEXT check.
        //
        // Trade-off: we lose the wire-level streaming pipeline (the UI won't
        // see tokens until the model finishes emitting them). In practice
        // this is fine — chat responses are short enough that draining
        // completes in a few seconds, and the alternative (streaming but no
        // retry on empty) is the exact bug we are fixing.
        let mut stream = response.bytes_stream();
        let mut byte_buffer: Vec<u8> = Vec::new();
        let mut active_tool_calls: std::collections::HashMap<usize, ToolCall> =
            std::collections::HashMap::new();
        let mut drained: Vec<LlmResponseChunk> = Vec::new();
        let mut total_text_len: usize = 0;
        const MAX_LINE_BUFFER: usize = 1_048_576;

        while let Some(chunk_result) = stream.next().await {
            let chunk = chunk_result.map_err(|e| format!("Stream error: {}", e))?;
            if byte_buffer.len() + chunk.len() > MAX_LINE_BUFFER {
                return Err("SSE stream partial line exceeded 1MB — aborting".to_string());
            }
            byte_buffer.extend_from_slice(&chunk);

            while let Some(newline_pos) = byte_buffer.iter().position(|&b| b == b'\n') {
                let line_bytes: Vec<u8> = byte_buffer.drain(..=newline_pos).collect();
                let mut end = line_bytes.len() - 1;
                if end > 0 && line_bytes[end - 1] == b'\r' {
                    end -= 1;
                }
                let line = String::from_utf8_lossy(&line_bytes[..end]);

                let Some(data) = line.strip_prefix("data: ") else {
                    continue;
                };
                let data = data.trim();
                if data == "[DONE]" {
                    continue;
                }
                let json = match serde_json::from_str::<Value>(data) {
                    Ok(v) => v,
                    Err(e) => {
                        tracing::warn!(
                            "SSE JSON parse error: {} — data: {}",
                            e,
                            &data[..data.len().min(200)]
                        );
                        continue;
                    }
                };
                let Some(choices) = json["choices"].as_array() else {
                    continue;
                };
                let Some(choice) = choices.first() else {
                    continue;
                };
                let delta = &choice["delta"];

                // P4 — skip thought-only chunks. Gemini 3.x emits chunks
                // carrying only `extra_content.google.thought_signature` and
                // neither text nor tool_calls. Without this guard they get
                // silently ignored (harmless) but ALSO any incidental
                // `content: ""` in the same delta was already dropped by the
                // empty-string check below, so this is defense in depth.
                let has_thought = delta
                    .get("extra_content")
                    .and_then(|e| e.get("google"))
                    .and_then(|g| g.get("thought_signature"))
                    .is_some();
                let has_content = delta
                    .get("content")
                    .and_then(|c| c.as_str())
                    .is_some_and(|s| !s.is_empty());
                let has_tool_calls = delta
                    .get("tool_calls")
                    .and_then(|a| a.as_array())
                    .is_some_and(|a| !a.is_empty());
                if has_thought && !has_content && !has_tool_calls {
                    continue;
                }

                if let Some(content) = delta["content"].as_str() {
                    if !content.is_empty() {
                        total_text_len += content.len();
                        drained.push(LlmResponseChunk::Text(content.to_string()));
                    }
                }

                if let Some(tool_calls) = delta["tool_calls"].as_array() {
                    for tc in tool_calls {
                        if let Some(index) = tc["index"].as_u64() {
                            let idx = index as usize;

                            if let Some(id) = tc["id"].as_str() {
                                active_tool_calls.insert(
                                    idx,
                                    ToolCall {
                                        id: id.to_string(),
                                        name: tc["function"]["name"]
                                            .as_str()
                                            .unwrap_or_default()
                                            .to_string(),
                                        arguments: String::new(),
                                    },
                                );
                            }

                            if let Some(active_tc) = active_tool_calls.get_mut(&idx) {
                                if let Some(args) = tc["function"]["arguments"].as_str() {
                                    active_tc.arguments.push_str(args);
                                }
                            }
                        }
                    }
                }
            }
        }

        // Gather completed tool calls in index order.
        let mut indices: Vec<usize> = active_tool_calls.keys().copied().collect();
        indices.sort_unstable();
        let mut collected_tool_calls: Vec<ToolCall> = Vec::with_capacity(indices.len());
        for idx in indices {
            if let Some(tc) = active_tool_calls.remove(&idx) {
                collected_tool_calls.push(tc);
            }
        }

        // Empty-stream detection: both visible text AND tool_calls missing.
        // Signal the outer retry loop by returning Err.
        if total_text_len == 0 && collected_tool_calls.is_empty() {
            return Err(format!(
                "Empty stream (no text, no tool_calls) — model={}, effort={}",
                self.model, effort
            ));
        }

        for tc in collected_tool_calls {
            drained.push(LlmResponseChunk::ToolCall(tc));
        }

        // Replay the drained chunks on a channel so we expose the same
        // `Stream` interface as before. Buffer size matches the chunk count
        // exactly to avoid blocking.
        let (tx, rx) = mpsc::channel::<Result<LlmResponseChunk, String>>(drained.len().max(1));
        tokio::spawn(async move {
            for chunk in drained {
                if tx.send(Ok(chunk)).await.is_err() {
                    return;
                }
            }
        });

        Ok(Box::pin(ReceiverStream::new(rx)))
    }
}

fn retry_efforts_for(start_effort: &str) -> [&'static str; 3] {
    match start_effort {
        "high" => ["high", "medium", "low"],
        "medium" => ["medium", "low", "low"],
        "none" => ["none", "none", "none"],
        _ => ["low", "low", "low"],
    }
}

#[cfg(test)]
mod tests {
    use super::{defaults_for, retry_efforts_for};

    #[test]
    fn defaults_for_gemini_flash_tier() {
        assert_eq!(
            defaults_for("gemini-3.1-flash-lite-preview"),
            (32768, "low")
        );
        assert_eq!(defaults_for("gemini-3-flash-preview"), (32768, "low"));
        assert_eq!(defaults_for("gemini-2.5-flash"), (8192, "medium"));
    }

    #[test]
    fn defaults_for_gemini_pro_tier() {
        assert_eq!(defaults_for("gemini-3.1-pro-preview"), (16384, "medium"));
        assert_eq!(defaults_for("gemini-3-pro-preview"), (16384, "medium"));
        assert_eq!(defaults_for("gemini-2.5-pro"), (16384, "high"));
    }

    #[test]
    fn defaults_for_unknown_falls_back() {
        assert_eq!(defaults_for("gpt-4o-mini"), (8192, "medium"));
        assert_eq!(defaults_for(""), (8192, "medium"));
        assert_eq!(defaults_for("claude-opus-4-7"), (8192, "medium"));
    }

    #[test]
    fn defaults_are_case_insensitive() {
        assert_eq!(defaults_for("GEMINI-3-FLASH-PREVIEW"), (32768, "low"));
        assert_eq!(defaults_for("  Gemini-2.5-Pro  "), (16384, "high"));
    }

    #[test]
    fn retry_efforts_preserve_explicit_none() {
        assert_eq!(retry_efforts_for("none"), ["none", "none", "none"]);
        assert_eq!(retry_efforts_for("medium"), ["medium", "low", "low"]);
    }
}

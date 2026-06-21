//! MCP server loop: reads JSON-RPC messages, dispatches to handlers, responds.

use serde_json::{json, Value};
use tracing::{error, info};

use crate::backend::local::LocalBackend;
use crate::jsonrpc::{JsonRpcRequest, JsonRpcResponse};
use crate::transport::stdio::StdioTransport;
use crate::{prompts, resources, tools};

/// MCP protocol version.
const MCP_PROTOCOL_VERSION: &str = "2024-11-05";

/// Server info returned in the initialize response.
fn server_info() -> Value {
    json!({
        "name": "code-explorer",
        "version": env!("CARGO_PKG_VERSION")
    })
}

/// Start the MCP server on stdio transport.
///
/// Reads JSON-RPC messages from stdin, dispatches them, and writes
/// responses to stdout. Handles graceful shutdown on ctrl+c.
pub async fn start_mcp_server(mut backend: LocalBackend) -> crate::error::Result<()> {
    // Initialize backend (load registry)
    if let Err(e) = backend.init() {
        error!("Failed to initialize backend: {e}");
        // Continue anyway - some tools may still work
    }

    let mut transport = StdioTransport::new();

    info!("Code Explorer MCP server starting on stdio");

    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                info!("Received ctrl+c, shutting down MCP server");
                break;
            }
            message = transport.read_message() => {
                match message {
                    Ok(Some(raw)) => {
                        // Try to parse as a request
                        match serde_json::from_str::<JsonRpcRequest>(&raw) {
                            Ok(request) => {
                                let response = handle_request(&request, &mut backend).await;
                                let response_json = serde_json::to_string(&response)
                                    // Fallback must include "id":null to remain a valid
                                    // JSON-RPC 2.0 response object — strict clients drop
                                    // responses that omit it.
                                    .unwrap_or_else(|_| r#"{"jsonrpc":"2.0","id":null,"error":{"code":-32603,"message":"Serialization error"}}"#.to_string());
                                if let Err(e) = transport.send_message(&response_json).await {
                                    error!("Failed to send response: {e}");
                                    break;
                                }
                            }
                            Err(e) => {
                                // Check if it's a notification (no id field)
                                if let Ok(notif) = serde_json::from_str::<crate::jsonrpc::JsonRpcNotification>(&raw) {
                                    // Notifications don't require a response
                                    info!("Received notification: {}", notif.method);
                                } else {
                                    // Parse error
                                    let error_resp = JsonRpcResponse::error(
                                        Value::Null,
                                        -32700,
                                        format!("Parse error: {e}"),
                                        None,
                                    );
                                    let response_json = serde_json::to_string(&error_resp).unwrap_or_default();
                                    let _ = transport.send_message(&response_json).await;
                                }
                            }
                        }
                    }
                    Ok(None) => {
                        // EOF - stdin closed
                        info!("Stdin closed, shutting down MCP server");
                        break;
                    }
                    Err(e) => {
                        error!("Transport error: {e}");
                        break;
                    }
                }
            }
        }
    }

    Ok(())
}

/// Handle a single JSON-RPC request and return a response.
pub async fn handle_request(
    request: &JsonRpcRequest,
    backend: &mut LocalBackend,
) -> JsonRpcResponse {
    let id = request.id.clone();

    match request.method.as_str() {
        "initialize" => JsonRpcResponse::success(
            id,
            json!({
                "protocolVersion": MCP_PROTOCOL_VERSION,
                "capabilities": {
                    "tools": { "listChanged": false },
                    "resources": { "subscribe": false, "listChanged": false },
                    "prompts": { "listChanged": false }
                },
                "serverInfo": server_info()
            }),
        ),
        "initialized" => {
            // Client acknowledgment, no response needed for notification-style
            // but since this came as a request, send empty success
            JsonRpcResponse::success(id, json!({}))
        }
        "tools/list" => JsonRpcResponse::success(id, tools::definitions::tools_list_json()),
        "tools/call" => {
            let tool_name = request
                .params
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let arguments = request
                .params
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));

            match backend.call_tool(tool_name, &arguments).await {
                Ok(result) => JsonRpcResponse::success(id, result),
                Err(e) => JsonRpcResponse::error(id, e.error_code(), e.to_string(), None),
            }
        }
        "resources/list" => JsonRpcResponse::success(id, resources::resource_definitions()),
        "resources/read" => {
            let uri = request
                .params
                .get("uri")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            match resources::read_resource(uri, backend.registry()) {
                Some(content) => JsonRpcResponse::success(id, content),
                None => {
                    JsonRpcResponse::error(id, -32002, format!("Resource not found: {uri}"), None)
                }
            }
        }
        "prompts/list" => JsonRpcResponse::success(id, prompts::prompt_definitions()),
        "prompts/get" => {
            let prompt_name = request
                .params
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let arguments = request
                .params
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));

            match prompts::get_prompt(prompt_name, &arguments) {
                Some(prompt) => JsonRpcResponse::success(id, prompt),
                None => JsonRpcResponse::error(
                    id,
                    -32602,
                    format!("Unknown prompt: {prompt_name}"),
                    None,
                ),
            }
        }
        "ping" => JsonRpcResponse::success(id, json!({})),
        _ => JsonRpcResponse::method_not_found(id, &request.method),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_request(method: &str, params: Value) -> JsonRpcRequest {
        JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Value::Number(1.into()),
            method: method.to_string(),
            params,
        }
    }

    #[tokio::test]
    async fn test_handle_initialize() {
        let mut backend = LocalBackend::new();
        let req = make_request("initialize", json!({}));
        let resp = handle_request(&req, &mut backend).await;
        assert!(resp.result.is_some());
        let result = resp.result.unwrap();
        assert_eq!(result["protocolVersion"], MCP_PROTOCOL_VERSION);
        assert!(result["serverInfo"]["name"]
            .as_str()
            .unwrap()
            .contains("code-explorer"));
    }

    #[tokio::test]
    async fn test_handle_tools_list() {
        let mut backend = LocalBackend::new();
        let req = make_request("tools/list", json!({}));
        let resp = handle_request(&req, &mut backend).await;
        let result = resp.result.unwrap();
        let tools = result["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 31);
    }

    #[tokio::test]
    async fn test_handle_resources_list() {
        let mut backend = LocalBackend::new();
        let req = make_request("resources/list", json!({}));
        let resp = handle_request(&req, &mut backend).await;
        assert!(resp.result.is_some());
    }

    #[tokio::test]
    async fn test_handle_prompts_list() {
        let mut backend = LocalBackend::new();
        let req = make_request("prompts/list", json!({}));
        let resp = handle_request(&req, &mut backend).await;
        let result = resp.result.unwrap();
        let prompts = result["prompts"].as_array().unwrap();
        assert_eq!(prompts.len(), 6);
    }

    #[tokio::test]
    async fn test_handle_unknown_method() {
        let mut backend = LocalBackend::new();
        let req = make_request("unknown/method", json!({}));
        let resp = handle_request(&req, &mut backend).await;
        assert!(resp.error.is_some());
        assert_eq!(resp.error.unwrap().code, -32601);
    }

    #[tokio::test]
    async fn test_handle_ping() {
        let mut backend = LocalBackend::new();
        let req = make_request("ping", json!({}));
        let resp = handle_request(&req, &mut backend).await;
        assert!(resp.result.is_some());
    }

    #[tokio::test]
    async fn test_handle_resource_read() {
        let mut backend = LocalBackend::new();
        let req = make_request("resources/read", json!({"uri": "code-explorer://version"}));
        let resp = handle_request(&req, &mut backend).await;
        assert!(resp.result.is_some());
    }

    #[tokio::test]
    async fn test_handle_prompt_get() {
        let mut backend = LocalBackend::new();
        let req = make_request(
            "prompts/get",
            json!({
                "name": "detect_impact",
                "arguments": {"target": "UserService"}
            }),
        );
        let resp = handle_request(&req, &mut backend).await;
        assert!(resp.result.is_some());
    }
}

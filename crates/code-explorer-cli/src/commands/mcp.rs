//! The `mcp` command: starts the MCP server on stdio transport.

use code_explorer_mcp::backend::local::LocalBackend;
use code_explorer_mcp::server::start_mcp_server;

pub async fn run() -> anyhow::Result<()> {
    let backend = LocalBackend::new();
    start_mcp_server(backend)
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    Ok(())
}

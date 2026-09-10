//! Stdio transport with auto-detect framing.
//!
//! Supports two framing modes:
//! - ContentLength: HTTP-style `Content-Length: N\r\n\r\n{json}` framing
//! - Newline: Simple newline-delimited JSON
//!
//! The framing mode is auto-detected from the first message received.

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::error::{McpError, Result};

/// Framing mode for stdio transport.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StdioFraming {
    /// HTTP-style Content-Length framing.
    ContentLength,
    /// Newline-delimited JSON.
    Newline,
}

/// Stdio transport for MCP server communication.
pub struct StdioTransport {
    reader: BufReader<tokio::io::Stdin>,
    writer: tokio::io::Stdout,
    framing: Option<StdioFraming>,
}

impl Default for StdioTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl StdioTransport {
    /// Create a new stdio transport.
    pub fn new() -> Self {
        Self {
            reader: BufReader::new(tokio::io::stdin()),
            writer: tokio::io::stdout(),
            framing: None,
        }
    }

    /// Read the next JSON message from stdin.
    ///
    /// Auto-detects framing on the first message:
    /// - If the first line starts with "Content-Length:", uses ContentLength framing
    /// - Otherwise, treats it as newline-delimited JSON
    pub async fn read_message(&mut self) -> Result<Option<String>> {
        loop {
            let mut line = String::new();
            let bytes_read = self
                .reader
                .read_line(&mut line)
                .await
                .map_err(|e| McpError::Transport(e.to_string()))?;

            if bytes_read == 0 {
                return Ok(None); // EOF
            }

            let trimmed = line.trim().to_string();
            if trimmed.is_empty() {
                // Skip empty lines and try again
                continue;
            }

            // Auto-detect framing on first message
            if self.framing.is_none() {
                if trimmed.starts_with("Content-Length:") {
                    self.framing = Some(StdioFraming::ContentLength);
                } else {
                    self.framing = Some(StdioFraming::Newline);
                }
            }

            // Safety: framing is always set in the block above when None
            match self.framing.expect("framing set above") {
                StdioFraming::ContentLength => {
                    if trimmed.starts_with("Content-Length:") {
                        return self.read_content_length_body(&trimmed).await;
                    }
                    // Skip non-header lines in content-length mode
                    continue;
                }
                StdioFraming::Newline => return Ok(Some(trimmed)),
            }
        }
    }

    /// Read the body of a Content-Length framed message.
    async fn read_content_length_body(&mut self, header: &str) -> Result<Option<String>> {
        let length_str = header
            .strip_prefix("Content-Length:")
            .ok_or_else(|| McpError::Transport("Invalid Content-Length header".into()))?
            .trim();

        let length: usize = length_str.parse().map_err(|_| {
            McpError::Transport(format!("Invalid Content-Length value: {length_str}"))
        })?;

        const MAX_MESSAGE_SIZE: usize = 64 * 1024 * 1024;
        if length > MAX_MESSAGE_SIZE {
            return Err(McpError::Transport(format!(
                "Content-Length {length} exceeds maximum allowed size of {MAX_MESSAGE_SIZE} bytes"
            )));
        }

        // LSP-style framing allows additional headers (e.g. `Content-Type`)
        // before the blank `\r\n\r\n` separator. Read header lines until we
        // hit a blank line, otherwise an extra header would be consumed as
        // the separator and `read_exact` would read mid-message, corrupting
        // the rest of the session.
        loop {
            let mut header_line = String::new();
            let n = self
                .reader
                .read_line(&mut header_line)
                .await
                .map_err(|e| McpError::Transport(e.to_string()))?;
            if n == 0 {
                return Err(McpError::Transport(
                    "Unexpected EOF while reading message headers".into(),
                ));
            }
            if header_line.trim().is_empty() {
                break;
            }
            // Other headers (Content-Type, etc.) are silently ignored.
        }

        // Read exactly `length` bytes
        let mut body = vec![0u8; length];
        tokio::io::AsyncReadExt::read_exact(&mut self.reader, &mut body)
            .await
            .map_err(|e| McpError::Transport(e.to_string()))?;

        let message = String::from_utf8(body).map_err(|e| McpError::Transport(e.to_string()))?;

        Ok(Some(message))
    }

    /// Send a JSON message to stdout with appropriate framing.
    pub async fn send_message(&mut self, message: &str) -> Result<()> {
        let framing = self.framing.unwrap_or(StdioFraming::Newline);

        match framing {
            StdioFraming::ContentLength => {
                let header = format!("Content-Length: {}\r\n\r\n", message.len());
                self.writer
                    .write_all(header.as_bytes())
                    .await
                    .map_err(|e| McpError::Transport(e.to_string()))?;
                self.writer
                    .write_all(message.as_bytes())
                    .await
                    .map_err(|e| McpError::Transport(e.to_string()))?;
            }
            StdioFraming::Newline => {
                self.writer
                    .write_all(message.as_bytes())
                    .await
                    .map_err(|e| McpError::Transport(e.to_string()))?;
                self.writer
                    .write_all(b"\n")
                    .await
                    .map_err(|e| McpError::Transport(e.to_string()))?;
            }
        }

        self.writer
            .flush()
            .await
            .map_err(|e| McpError::Transport(e.to_string()))?;

        Ok(())
    }

    /// Get the detected framing mode.
    pub fn framing(&self) -> Option<StdioFraming> {
        self.framing
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_framing_enum() {
        assert_ne!(StdioFraming::ContentLength, StdioFraming::Newline);
    }
}

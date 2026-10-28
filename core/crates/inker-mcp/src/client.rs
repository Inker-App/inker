//! A minimal MCP client, for the CLI's `list`/`resolve`/`describe` against a
//! running app. Line-delimited JSON-RPC over any byte stream; no rmcp client.

use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};

/// Protocol revision the client asks for. The server negotiates down if needed.
const PROTOCOL_VERSION: &str = "2025-11-25";

/// Client errors.
#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("protocol: {0}")]
    Protocol(String),
}

/// What a tool call returned.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolOutput {
    pub text: String,
    pub structured: Option<Value>,
    pub is_error: bool,
}

/// A connected, initialized client.
#[derive(Debug)]
pub struct McpClient<S> {
    stream: BufReader<S>,
    next_id: u64,
    instructions: Option<String>,
}

impl<S: AsyncRead + AsyncWrite + Unpin> McpClient<S> {
    /// Run the `initialize` handshake as `client_name`.
    pub async fn connect(stream: S, client_name: &str) -> Result<Self, ClientError> {
        let mut c = Self {
            stream: BufReader::new(stream),
            next_id: 1,
            instructions: None,
        };
        let init = c
            .request(
                "initialize",
                json!({
                    "protocolVersion": PROTOCOL_VERSION,
                    "capabilities": {},
                    "clientInfo": {"name": client_name, "version": env!("CARGO_PKG_VERSION")}
                }),
            )
            .await?;
        c.instructions = init
            .get("instructions")
            .and_then(Value::as_str)
            .map(str::to_string);
        c.send(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}))
            .await?;
        Ok(c)
    }

    /// The server's `instructions`, from the handshake.
    pub fn instructions(&self) -> Option<&str> {
        self.instructions.as_deref()
    }

    /// Call a tool. A tool-level error (`isError`) is `Ok` with `is_error`
    /// set; a JSON-RPC error is `Err(Protocol)`.
    pub async fn call_tool(&mut self, name: &str, args: Value) -> Result<ToolOutput, ClientError> {
        let r = self
            .request("tools/call", json!({"name": name, "arguments": args}))
            .await?;
        let text = r["content"]
            .as_array()
            .map(|blocks| {
                blocks
                    .iter()
                    .filter_map(|b| b["text"].as_str())
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_default();
        Ok(ToolOutput {
            text,
            structured: r.get("structuredContent").cloned(),
            is_error: r["isError"] == json!(true),
        })
    }

    async fn send(&mut self, v: &Value) -> Result<(), ClientError> {
        let mut line = serde_json::to_vec(v).map_err(|e| ClientError::Protocol(e.to_string()))?;
        line.push(b'\n');
        let w = self.stream.get_mut();
        w.write_all(&line).await?;
        w.flush().await?;
        Ok(())
    }

    async fn request(&mut self, method: &str, params: Value) -> Result<Value, ClientError> {
        let id = self.next_id;
        self.next_id += 1;
        self.send(&json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}))
            .await?;
        let mut line = String::new();
        loop {
            line.clear();
            if self.stream.read_line(&mut line).await? == 0 {
                return Err(ClientError::Protocol("server closed the connection".into()));
            }
            let Ok(v) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            if v.get("id") != Some(&json!(id)) {
                continue; // notifications or server-to-client requests
            }
            if let Some(err) = v.get("error") {
                let msg = err["message"].as_str().unwrap_or("unknown error");
                return Err(ClientError::Protocol(msg.to_string()));
            }
            return Ok(v.get("result").cloned().unwrap_or(Value::Null));
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use serde_json::json;
    use tokio_util::sync::CancellationToken;

    use super::*;
    use crate::{Backend, FakeBackend, ServerOptions, serve_stream};

    async fn connected(fake: Arc<FakeBackend>) -> McpClient<tokio::io::DuplexStream> {
        let (a, b) = tokio::io::duplex(1 << 16);
        let backend: Arc<dyn Backend> = fake;
        tokio::spawn(serve_stream(
            backend,
            b,
            ServerOptions::default(),
            CancellationToken::new(),
        ));
        McpClient::connect(a, "inker-cli").await.unwrap()
    }

    #[tokio::test]
    async fn handshake_gets_instructions() {
        let c = connected(Arc::new(FakeBackend::sample())).await;
        assert_eq!(c.instructions(), Some(crate::instructions()));
    }

    #[tokio::test]
    async fn calls_tools_and_reports_errors() {
        let fake = Arc::new(FakeBackend::sample());
        let mut c = connected(fake.clone()).await;
        let out = c
            .call_tool("resolve", json!({"handle": "w2-acme-2025"}))
            .await
            .unwrap();
        assert!(!out.is_error);
        assert!(out.text.contains("[EIN_1]"));
        assert_eq!(out.structured.unwrap()["type"], json!("document"));
        assert_eq!(fake.reads()[0].0, "inker-cli");

        let out = c
            .call_tool("resolve", json!({"handle": "1098-mortgage-2025"}))
            .await
            .unwrap();
        assert!(out.is_error);
        assert!(out.text.starts_with("not ready"));

        let err = c.call_tool("bogus", json!({})).await.unwrap_err();
        assert!(matches!(err, ClientError::Protocol(_)));
    }

    #[tokio::test]
    async fn closed_stream_is_an_error() {
        let (a, b) = tokio::io::duplex(64);
        drop(b);
        assert!(McpClient::connect(a, "x").await.is_err());
    }
}

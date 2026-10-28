//! A tiny line-delimited JSON-RPC client for tests.
#![allow(dead_code)]

use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader, Lines};

pub struct RawClient<R, W> {
    lines: Lines<BufReader<R>>,
    writer: W,
    next_id: i64,
    /// Every line the server sent, for leak checks.
    pub transcript: Vec<String>,
}

impl<R: AsyncRead + Unpin, W: AsyncWrite + Unpin> RawClient<R, W> {
    pub fn new(reader: R, writer: W) -> Self {
        Self {
            lines: BufReader::new(reader).lines(),
            writer,
            next_id: 1,
            transcript: vec![],
        }
    }

    pub async fn send(&mut self, v: Value) {
        let mut line = serde_json::to_string(&v).unwrap();
        line.push('\n');
        self.writer.write_all(line.as_bytes()).await.unwrap();
        self.writer.flush().await.unwrap();
    }

    pub async fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        self.send(json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}))
            .await;
        loop {
            let line =
                tokio::time::timeout(std::time::Duration::from_secs(10), self.lines.next_line())
                    .await
                    .expect("timed out waiting for a response")
                    .unwrap()
                    .expect("server closed the stream");
            self.transcript.push(line.clone());
            let v: Value = serde_json::from_str(&line).unwrap();
            if v.get("id") == Some(&json!(id)) {
                return v;
            }
        }
    }

    pub async fn initialize(&mut self, client_name: &str) -> Value {
        let r = self
            .request(
                "initialize",
                json!({
                    "protocolVersion": "2025-11-25",
                    "capabilities": {},
                    "clientInfo": {"name": client_name, "version": "1.0.0"}
                }),
            )
            .await;
        self.send(json!({"jsonrpc":"2.0","method":"notifications/initialized"}))
            .await;
        r
    }

    pub async fn call(&mut self, tool: &str, args: Value) -> Value {
        self.request("tools/call", json!({"name": tool, "arguments": args}))
            .await
    }
}

/// Text of the first content block of a tools/call response.
pub fn text(resp: &Value) -> String {
    resp["result"]["content"][0]["text"]
        .as_str()
        .unwrap_or_default()
        .to_string()
}

pub fn is_error(resp: &Value) -> bool {
    resp["result"]["isError"] == json!(true)
}

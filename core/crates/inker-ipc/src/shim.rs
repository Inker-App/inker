//! `inker mcp`: stdio <-> socket, byte for byte.
//!
//! The shim never parses traffic. The one exception is the failure path:
//! when INKER is not running it peeks at the agent's first line to put that
//! request's id on the error, so clients match it to their `initialize`.

use std::path::Path;
use std::time::{Duration, Instant};

use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

/// The message MCP clients show when the app is not running.
pub const NOT_RUNNING_MESSAGE: &str =
    "INKER is not running. Open INKER from your menu bar and try again.";

/// The app's bundle identifier, for `open -b`.
pub const BUNDLE_ID: &str = "sh.inker.app";

/// Launches the app. Returns once the launch was requested, not when it is up.
pub type Launcher = Box<dyn Fn() -> std::io::Result<()> + Send + Sync>;

/// How the shim behaves when the socket is absent.
pub struct ShimOptions {
    /// `None` means do not try to launch anything.
    pub launcher: Option<Launcher>,
    /// How long to poll for the socket after launching.
    pub launch_timeout: Duration,
    /// How long to wait for the agent's first request when replying with the
    /// not-running error, so the error can carry that request's id.
    pub first_request_timeout: Duration,
}

impl std::fmt::Debug for ShimOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ShimOptions")
            .field("launcher", &self.launcher.is_some())
            .field("launch_timeout", &self.launch_timeout)
            .finish_non_exhaustive()
    }
}

impl Default for ShimOptions {
    /// On macOS, launch the app with `open -b`; elsewhere, do not launch.
    fn default() -> Self {
        Self {
            launcher: platform_launcher(),
            launch_timeout: Duration::from_secs(10),
            first_request_timeout: Duration::from_secs(2),
        }
    }
}

#[cfg(target_os = "macos")]
fn platform_launcher() -> Option<Launcher> {
    Some(Box::new(|| {
        std::process::Command::new("open")
            .args(["-g", "-b", BUNDLE_ID])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .and_then(|s| {
                if s.success() {
                    Ok(())
                } else {
                    Err(std::io::Error::other(format!("open -b {BUNDLE_ID} failed")))
                }
            })
    }))
}

#[cfg(not(target_os = "macos"))]
fn platform_launcher() -> Option<Launcher> {
    None
}

/// Shim errors.
#[derive(Debug, thiserror::Error)]
pub enum ShimError {
    /// The app is not running; the JSON-RPC error has already been written.
    #[error("{NOT_RUNNING_MESSAGE}")]
    NotRunning,
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

/// The JSON-RPC error line written when INKER is not running.
pub fn not_running_response(id: Value) -> String {
    // Written by hand to keep the documented key order.
    let message = Value::String(NOT_RUNNING_MESSAGE.to_string());
    format!(
        "{{\"jsonrpc\":\"2.0\",\"id\":{id},\"error\":{{\"code\":-32000,\"message\":{message}}}}}\n"
    )
}

async fn connect_or_launch(socket_path: &Path, options: &ShimOptions) -> Option<UnixStream> {
    if let Ok(s) = UnixStream::connect(socket_path).await {
        return Some(s);
    }
    let launcher = options.launcher.as_ref()?;
    if let Err(e) = launcher() {
        tracing::warn!(error = %e, "could not launch INKER");
        return None;
    }
    let deadline = Instant::now() + options.launch_timeout;
    while Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(100)).await;
        if let Ok(s) = UnixStream::connect(socket_path).await {
            return Some(s);
        }
    }
    None
}

/// Forward `input` to the socket and the socket to `output` until either closes.
pub async fn run_with<R, W>(
    socket_path: &Path,
    input: R,
    mut output: W,
    options: ShimOptions,
) -> Result<(), ShimError>
where
    R: AsyncRead + Unpin + Send,
    W: AsyncWrite + Unpin + Send,
{
    let Some(stream) = connect_or_launch(socket_path, &options).await else {
        let mut reader = BufReader::new(input);
        let mut first = String::new();
        let id =
            match tokio::time::timeout(options.first_request_timeout, reader.read_line(&mut first))
                .await
            {
                Ok(Ok(n)) if n > 0 => serde_json::from_str::<Value>(&first)
                    .ok()
                    .and_then(|v| v.get("id").cloned())
                    .unwrap_or(Value::Null),
                _ => Value::Null,
            };
        output
            .write_all(not_running_response(id).as_bytes())
            .await?;
        output.flush().await?;
        return Err(ShimError::NotRunning);
    };
    let (sock_r, sock_w) = stream.into_split();
    let up = pump(input, sock_w, true);
    let down = pump(sock_r, &mut output, false);
    tokio::pin!(up, down);
    tokio::select! {
        r = &mut down => closed_ok(r)?,
        r = &mut up => {
            closed_ok(r)?;
            // The agent closed stdin. Let the host finish answering, briefly.
            match tokio::time::timeout(Duration::from_secs(2), &mut down).await {
                Ok(r) => closed_ok(r)?,
                Err(_) => tracing::debug!("host did not close after stdin EOF"),
            }
        }
    }
    Ok(())
}

/// A peer that hangs up abruptly (reset, broken pipe) has simply closed.
fn closed_ok(r: std::io::Result<()>) -> std::io::Result<()> {
    use std::io::ErrorKind::{BrokenPipe, ConnectionAborted, ConnectionReset};
    match r {
        Err(e) if matches!(e.kind(), BrokenPipe | ConnectionReset | ConnectionAborted) => {
            tracing::debug!("peer hung up");
            Ok(())
        }
        other => other,
    }
}

/// Copy until EOF, flushing after every chunk so JSON-RPC lines are not held
/// in a buffer. With `shutdown`, half-close the writer at EOF.
async fn pump<R, W>(mut r: R, mut w: W, shutdown: bool) -> std::io::Result<()>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = r.read(&mut buf).await?;
        if n == 0 {
            if shutdown {
                let _ = w.shutdown().await;
            }
            return Ok(());
        }
        w.write_all(&buf[..n]).await?;
        w.flush().await?;
    }
}

/// `inker mcp`: run against this process's stdio with platform defaults.
pub async fn run(socket_path: &Path) -> Result<(), ShimError> {
    run_with(
        socket_path,
        tokio::io::stdin(),
        tokio::io::stdout(),
        ShimOptions::default(),
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn not_running_line_is_the_documented_json() {
        assert_eq!(
            not_running_response(Value::Null),
            "{\"jsonrpc\":\"2.0\",\"id\":null,\"error\":{\"code\":-32000,\"message\":\"INKER is not running. Open INKER from your menu bar and try again.\"}}\n"
        );
    }

    #[test]
    fn default_launcher_is_macos_only() {
        assert_eq!(
            ShimOptions::default().launcher.is_some(),
            cfg!(target_os = "macos")
        );
        assert_eq!(
            ShimOptions::default().launch_timeout,
            Duration::from_secs(10)
        );
    }
}

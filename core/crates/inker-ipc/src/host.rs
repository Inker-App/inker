//! The socket host: owns `inker.sock` and serves MCP on each connection.
//!
//! The socket is the security boundary (stack-decision.md 3.2): it lives in a
//! `0700` directory and is `chmod 0600` right after `bind`, so only the user
//! can connect. The window between the two is covered by the directory mode.

use std::os::unix::fs::{DirBuilderExt, FileTypeExt, PermissionsExt};
use std::path::Path;
use std::sync::Arc;

use inker_mcp::{Backend, ServerOptions, Sessions};
use tokio::net::UnixListener;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

/// Host errors.
#[derive(Debug, thiserror::Error)]
pub enum HostError {
    #[error("another INKER is already listening on {0}")]
    AlreadyRunning(String),
    #[error("{0} exists and is not a socket; refusing to replace it")]
    NotASocket(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

/// Bind the socket: create the data dir (0700), clear a stale socket, bind,
/// and restrict the socket to the user (0600).
pub async fn bind(socket_path: &Path) -> Result<UnixListener, HostError> {
    let shown = socket_path.display().to_string();
    if let Some(dir) = socket_path
        .parent()
        .filter(|d| !d.as_os_str().is_empty() && !d.exists())
    {
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir)?;
    }
    match std::fs::symlink_metadata(socket_path) {
        Ok(meta) if meta.file_type().is_socket() => {
            if tokio::net::UnixStream::connect(socket_path).await.is_ok() {
                return Err(HostError::AlreadyRunning(shown));
            }
            tracing::info!("removing stale socket");
            std::fs::remove_file(socket_path)?;
        }
        Ok(_) => return Err(HostError::NotASocket(shown)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }
    let listener = UnixListener::bind(socket_path)?;
    std::fs::set_permissions(socket_path, std::fs::Permissions::from_mode(0o600))?;
    Ok(listener)
}

/// Serve until `ct` is cancelled. Removes the socket file on the way out.
pub async fn serve(
    socket_path: &Path,
    backend: Arc<dyn Backend>,
    options: ServerOptions,
    ct: CancellationToken,
) -> Result<(), HostError> {
    serve_with(socket_path, backend, options, None, ct).await
}

/// `serve`, registering every connection in `sessions` when given one, so
/// the app can show who is connected.
pub async fn serve_with(
    socket_path: &Path,
    backend: Arc<dyn Backend>,
    options: ServerOptions,
    sessions: Option<Arc<Sessions>>,
    ct: CancellationToken,
) -> Result<(), HostError> {
    let listener = bind(socket_path).await?;
    tracing::info!("listening");
    let sessions_registry = sessions;
    let mut sessions = JoinSet::new();
    let result = loop {
        tokio::select! {
            _ = ct.cancelled() => break Ok(()),
            accepted = listener.accept() => match accepted {
                Ok((stream, _)) => {
                    let (backend, session_ct) = (backend.clone(), ct.child_token());
                    let registry = sessions_registry.clone();
                    sessions.spawn(async move {
                        let served = match registry {
                            Some(r) => inker_mcp::serve_stream_in(backend, stream, options, r, session_ct).await,
                            None => inker_mcp::serve_stream(backend, stream, options, session_ct).await,
                        };
                        if let Err(e) = served {
                            tracing::warn!(error = %e, "session ended with an error");
                        }
                    });
                    tracing::debug!(open = sessions.len(), "session opened");
                }
                Err(e) => break Err(HostError::Io(e)),
            },
            Some(_) = sessions.join_next(), if !sessions.is_empty() => {}
        }
    };
    drop(listener);
    let _ = std::fs::remove_file(socket_path);
    // Child tokens are cancelled with `ct`; give sessions a moment to close.
    let drain = async { while sessions.join_next().await.is_some() {} };
    if tokio::time::timeout(std::time::Duration::from_secs(2), drain)
        .await
        .is_err()
    {
        sessions.abort_all();
    }
    tracing::info!("stopped");
    result
}

/// Serve until SIGINT or SIGTERM (or `ct`).
pub async fn serve_until_signal(
    socket_path: &Path,
    backend: Arc<dyn Backend>,
    options: ServerOptions,
    ct: CancellationToken,
) -> Result<(), HostError> {
    let trigger = ct.clone();
    let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    tokio::spawn(async move {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = term.recv() => {}
            _ = trigger.cancelled() => {}
        }
        trigger.cancel();
    });
    serve(socket_path, backend, options, ct).await
}

#![allow(clippy::unwrap_used, clippy::expect_used)]
mod common;

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use common::{RawClient, text};
use inker_ipc::host::{self, HostError};
use inker_mcp::{Backend, FakeBackend, ServerOptions};
use serde_json::json;
use tokio::net::UnixStream;
use tokio_util::sync::CancellationToken;

fn backend() -> Arc<dyn Backend> {
    Arc::new(FakeBackend::sample())
}

async fn wait_for(path: &Path) {
    for _ in 0..200 {
        if UnixStream::connect(path).await.is_ok() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("socket never came up at {path:?}");
}

fn start(
    path: PathBuf,
) -> (
    CancellationToken,
    tokio::task::JoinHandle<Result<(), HostError>>,
) {
    let ct = CancellationToken::new();
    let c2 = ct.clone();
    let h =
        tokio::spawn(
            async move { host::serve(&path, backend(), ServerOptions::default(), c2).await },
        );
    (ct, h)
}

#[tokio::test]
async fn socket_is_0600_in_a_0700_dir() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("data");
    let sock = dir.join("inker.sock");
    let (ct, h) = start(sock.clone());
    wait_for(&sock).await;
    assert_eq!(
        std::fs::metadata(&sock).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert_eq!(
        std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777,
        0o700
    );
    ct.cancel();
    h.await.unwrap().unwrap();
    assert!(!sock.exists(), "socket removed on shutdown");
}

#[tokio::test]
async fn serves_concurrent_connections() {
    let tmp = tempfile::tempdir().unwrap();
    let sock = tmp.path().join("inker.sock");
    let (ct, _h) = start(sock.clone());
    wait_for(&sock).await;
    let mut clients = Vec::new();
    for name in ["claude-code", "codex", "cursor"] {
        let (r, w) = UnixStream::connect(&sock).await.unwrap().into_split();
        let mut c = RawClient::new(r, w);
        c.initialize(name).await;
        clients.push(c);
    }
    // Interleave calls across the three live sessions.
    for c in clients.iter_mut().rev() {
        let r = c
            .call("resolve", json!({"handle": "🐙/w2-acme-2025"}))
            .await;
        assert!(text(&r).contains("[SSN_1]"));
    }
    ct.cancel();
}

#[tokio::test]
async fn removes_a_stale_socket() {
    let tmp = tempfile::tempdir().unwrap();
    let sock = tmp.path().join("inker.sock");
    drop(std::os::unix::net::UnixListener::bind(&sock).unwrap()); // file stays, nobody listens
    assert!(sock.exists());
    let (ct, _h) = start(sock.clone());
    wait_for(&sock).await;
    let (r, w) = UnixStream::connect(&sock).await.unwrap().into_split();
    let mut c = RawClient::new(r, w);
    assert!(c.initialize("x").await["result"].is_object());
    ct.cancel();
}

#[tokio::test]
async fn refuses_to_steal_a_live_socket() {
    let tmp = tempfile::tempdir().unwrap();
    let sock = tmp.path().join("inker.sock");
    let _live = tokio::net::UnixListener::bind(&sock).unwrap();
    let err = host::serve(
        &sock,
        backend(),
        ServerOptions::default(),
        CancellationToken::new(),
    )
    .await
    .unwrap_err();
    assert!(matches!(err, HostError::AlreadyRunning(_)), "{err}");
    assert!(sock.exists());
}

#[tokio::test]
async fn refuses_to_replace_a_regular_file() {
    let tmp = tempfile::tempdir().unwrap();
    let sock = tmp.path().join("inker.sock");
    std::fs::write(&sock, "not a socket").unwrap();
    let err = host::serve(
        &sock,
        backend(),
        ServerOptions::default(),
        CancellationToken::new(),
    )
    .await
    .unwrap_err();
    assert!(matches!(err, HostError::NotASocket(_)), "{err}");
    assert_eq!(std::fs::read_to_string(&sock).unwrap(), "not a socket");
}

#[tokio::test]
async fn cancel_ends_open_sessions() {
    let tmp = tempfile::tempdir().unwrap();
    let sock = tmp.path().join("inker.sock");
    let (ct, h) = start(sock.clone());
    wait_for(&sock).await;
    let (r, w) = UnixStream::connect(&sock).await.unwrap().into_split();
    let mut c = RawClient::new(r, w);
    c.initialize("x").await;
    ct.cancel();
    tokio::time::timeout(Duration::from_secs(5), h)
        .await
        .expect("host did not stop")
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn serve_until_signal_also_stops_on_cancel() {
    let tmp = tempfile::tempdir().unwrap();
    let sock = tmp.path().join("inker.sock");
    let ct = CancellationToken::new();
    let (p, c2) = (sock.clone(), ct.clone());
    let h = tokio::spawn(async move {
        host::serve_until_signal(&p, backend(), ServerOptions::default(), c2).await
    });
    wait_for(&sock).await;
    ct.cancel();
    tokio::time::timeout(Duration::from_secs(5), h)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(!sock.exists());
}

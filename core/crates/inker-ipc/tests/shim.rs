#![allow(clippy::unwrap_used, clippy::expect_used)]
mod common;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use common::{RawClient, is_error, text};
use inker_ipc::host;
use inker_ipc::shim::{self, NOT_RUNNING_MESSAGE, ShimError, ShimOptions};
use inker_mcp::{Backend, FakeBackend, ServerOptions};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio_util::sync::CancellationToken;

async fn wait_for(path: &Path) {
    for _ in 0..500 {
        if UnixStream::connect(path).await.is_ok() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("socket never came up");
}

fn start_host(path: PathBuf, fake: Arc<FakeBackend>) -> CancellationToken {
    let ct = CancellationToken::new();
    let c2 = ct.clone();
    let backend: Arc<dyn Backend> = fake;
    tokio::spawn(async move { host::serve(&path, backend, ServerOptions::default(), c2).await });
    ct
}

fn quick() -> ShimOptions {
    ShimOptions {
        launcher: None,
        launch_timeout: Duration::from_millis(300),
        first_request_timeout: Duration::from_millis(300),
    }
}

/// Run the shim in-process with piped stdio. Returns the agent-side client
/// and the shim task.
fn start_shim(
    sock: PathBuf,
    options: ShimOptions,
) -> (
    RawClient<tokio::io::DuplexStream, tokio::io::DuplexStream>,
    tokio::task::JoinHandle<Result<(), ShimError>>,
) {
    let (agent_out, shim_in) = tokio::io::duplex(1 << 16); // agent -> shim stdin
    let (shim_out, agent_in) = tokio::io::duplex(1 << 16); // shim stdout -> agent
    let task = tokio::spawn(async move { shim::run_with(&sock, shim_in, shim_out, options).await });
    (RawClient::new(agent_in, agent_out), task)
}

#[tokio::test]
async fn forwards_initialize_list_and_resolve() {
    let tmp = tempfile::tempdir().unwrap();
    let sock = tmp.path().join("inker.sock");
    let fake = Arc::new(FakeBackend::sample());
    let ct = start_host(sock.clone(), fake.clone());
    wait_for(&sock).await;

    let (mut agent, task) = start_shim(sock, quick());
    let init = agent.initialize("claude-code").await;
    assert_eq!(
        init["result"]["instructions"],
        json!(inker_mcp::instructions())
    );
    let tools = agent.request("tools/list", json!({})).await;
    assert_eq!(tools["result"]["tools"].as_array().unwrap().len(), 4);
    let r = agent
        .call("resolve", json!({"handle": "🐙/w2-acme-2025"}))
        .await;
    assert!(text(&r).contains("Wages, tips, other compensation: 58,250.00"));
    let r = agent
        .call("resolve", json!({"handle": "🐙/1099-int-bank-2025"}))
        .await;
    assert!(is_error(&r));
    assert_eq!(fake.reads()[0].0, "Claude Code");

    // Agent closes stdin: the shim exits cleanly.
    drop(agent);
    tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .expect("shim hung")
        .unwrap()
        .unwrap();
    ct.cancel();
}

#[tokio::test]
async fn exits_when_the_host_goes_away() {
    let tmp = tempfile::tempdir().unwrap();
    let sock = tmp.path().join("inker.sock");
    let ct = start_host(sock.clone(), Arc::new(FakeBackend::sample()));
    wait_for(&sock).await;
    let (mut agent, task) = start_shim(sock, quick());
    agent.initialize("x").await;
    ct.cancel();
    tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .expect("shim hung")
        .unwrap()
        .unwrap();
    drop(agent);
}

#[tokio::test]
async fn forwards_bytes_unparsed() {
    // A raw echo server stands in for the host: whatever the agent sends,
    // including non-JSON, must arrive unchanged, and replies must come back.
    let tmp = tempfile::tempdir().unwrap();
    let sock = tmp.path().join("echo.sock");
    let listener = tokio::net::UnixListener::bind(&sock).unwrap();
    tokio::spawn(async move {
        let (mut s, _) = listener.accept().await.unwrap();
        let (mut r, mut w) = s.split();
        tokio::io::copy(&mut r, &mut w).await.unwrap();
    });
    let (mut agent_out, shim_in) = tokio::io::duplex(1024);
    let (shim_out, agent_in) = tokio::io::duplex(1024);
    let task = tokio::spawn(async move { shim::run_with(&sock, shim_in, shim_out, quick()).await });
    let payload = "not json \u{1F419} {\"a\":1}\n\0binary\n";
    agent_out.write_all(payload.as_bytes()).await.unwrap();
    let mut got = vec![0u8; payload.len()];
    let mut agent_in = agent_in;
    tokio::io::AsyncReadExt::read_exact(&mut agent_in, &mut got)
        .await
        .unwrap();
    assert_eq!(got, payload.as_bytes());
    drop(agent_out);
    tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
}

async fn read_error_line(agent_in: tokio::io::DuplexStream) -> Value {
    let mut lines = BufReader::new(agent_in).lines();
    let line = tokio::time::timeout(Duration::from_secs(5), lines.next_line())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    serde_json::from_str(&line).unwrap()
}

#[tokio::test]
async fn absent_socket_prints_jsonrpc_error_and_fails() {
    let tmp = tempfile::tempdir().unwrap();
    let sock = tmp.path().join("missing.sock");
    let (_agent_out, shim_in) = tokio::io::duplex(1024); // agent never writes
    let (shim_out, agent_in) = tokio::io::duplex(1024);
    let res = shim::run_with(&sock, shim_in, shim_out, quick()).await;
    assert!(matches!(res, Err(ShimError::NotRunning)));
    let v = read_error_line(agent_in).await;
    assert_eq!(
        v,
        json!({"jsonrpc":"2.0","id":null,"error":{"code":-32000,"message":NOT_RUNNING_MESSAGE}})
    );
}

#[tokio::test]
async fn absent_socket_error_carries_the_first_request_id() {
    let tmp = tempfile::tempdir().unwrap();
    let sock = tmp.path().join("missing.sock");
    let (mut agent_out, shim_in) = tokio::io::duplex(1024);
    let (shim_out, agent_in) = tokio::io::duplex(1024);
    agent_out
        .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":7,\"method\":\"initialize\",\"params\":{}}\n")
        .await
        .unwrap();
    let res = shim::run_with(&sock, shim_in, shim_out, quick()).await;
    assert!(matches!(res, Err(ShimError::NotRunning)));
    let v = read_error_line(agent_in).await;
    assert_eq!(v["id"], json!(7));
    assert_eq!(v["error"]["message"], json!(NOT_RUNNING_MESSAGE));
}

#[tokio::test]
async fn stale_socket_file_counts_as_absent() {
    let tmp = tempfile::tempdir().unwrap();
    let sock = tmp.path().join("inker.sock");
    drop(std::os::unix::net::UnixListener::bind(&sock).unwrap());
    let (_agent_out, shim_in) = tokio::io::duplex(1024);
    let (shim_out, agent_in) = tokio::io::duplex(1024);
    let res = shim::run_with(&sock, shim_in, shim_out, quick()).await;
    assert!(matches!(res, Err(ShimError::NotRunning)));
    assert_eq!(
        read_error_line(agent_in).await["error"]["code"],
        json!(-32000)
    );
}

#[tokio::test]
async fn launches_the_app_and_waits_for_the_socket() {
    let tmp = tempfile::tempdir().unwrap();
    let sock = tmp.path().join("inker.sock");
    let fake = Arc::new(FakeBackend::sample());
    let launched = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let (path2, fake2, flag) = (sock.clone(), fake.clone(), launched.clone());
    let rt = tokio::runtime::Handle::current();
    let options = ShimOptions {
        launcher: Some(Box::new(move || {
            flag.store(true, std::sync::atomic::Ordering::SeqCst);
            let (p, f) = (path2.clone(), fake2.clone());
            rt.spawn(async move {
                tokio::time::sleep(Duration::from_millis(200)).await; // the app takes a moment
                let backend: Arc<dyn Backend> = f;
                host::serve(
                    &p,
                    backend,
                    ServerOptions::default(),
                    CancellationToken::new(),
                )
                .await
            });
            Ok(())
        })),
        launch_timeout: Duration::from_secs(5),
        first_request_timeout: Duration::from_millis(300),
    };
    let (mut agent, _task) = start_shim(sock, options);
    let init = agent.initialize("claude-code").await;
    assert!(init["result"]["serverInfo"].is_object());
    assert!(launched.load(std::sync::atomic::Ordering::SeqCst));
}

#[tokio::test]
async fn launch_timeout_falls_back_to_the_error() {
    let tmp = tempfile::tempdir().unwrap();
    let sock = tmp.path().join("inker.sock");
    let options = ShimOptions {
        launcher: Some(Box::new(|| Ok(()))), // "launches" but nothing ever listens
        launch_timeout: Duration::from_millis(300),
        first_request_timeout: Duration::from_millis(100),
    };
    let (_agent_out, shim_in) = tokio::io::duplex(1024);
    let (shim_out, agent_in) = tokio::io::duplex(1024);
    let started = std::time::Instant::now();
    let res = shim::run_with(&sock, shim_in, shim_out, options).await;
    assert!(matches!(res, Err(ShimError::NotRunning)));
    assert!(started.elapsed() >= Duration::from_millis(300));
    assert_eq!(
        read_error_line(agent_in).await["error"]["code"],
        json!(-32000)
    );
}

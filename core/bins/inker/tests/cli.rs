#![allow(clippy::unwrap_used, clippy::expect_used)]
//! CLI tests. Every command runs with a temp HOME, a temp data dir and a
//! minimal PATH, so nothing here can touch the real agent configs.

use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use assert_cmd::cargo::cargo_bin;
use predicates::prelude::*;
use serde_json::{Value, json};
use tempfile::TempDir;

struct Sandbox {
    dir: TempDir,
}

impl Sandbox {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("home")).unwrap();
        Self { dir }
    }
    fn home(&self) -> PathBuf {
        self.dir.path().join("home")
    }
    fn data(&self) -> PathBuf {
        self.dir.path().join("data")
    }
    fn socket(&self) -> PathBuf {
        self.data().join("inker.sock")
    }
    fn cmd(&self) -> assert_cmd::Command {
        let mut c = assert_cmd::Command::new(cargo_bin("inker"));
        self.scrub(c.env_clear());
        c
    }
    fn std_cmd(&self) -> Command {
        let mut c = Command::new(cargo_bin("inker"));
        c.env_clear();
        c.env("HOME", self.home())
            .env("INKER_DATA_DIR", self.data())
            .env("PATH", "/usr/bin:/bin");
        c
    }
    fn scrub<'a>(&self, c: &'a mut assert_cmd::Command) -> &'a mut assert_cmd::Command {
        c.env("HOME", self.home())
            .env("INKER_DATA_DIR", self.data())
            .env("PATH", "/usr/bin:/bin")
    }
    /// `inker serve --fake` in the background, waiting for the socket.
    fn serve(&self, extra: &[&str]) -> Child {
        let child = self
            .std_cmd()
            .args(["serve", "--fake"])
            .args(extra)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        wait_for_socket(&self.socket());
        child
    }
}

fn wait_for_socket(p: &Path) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if std::os::unix::net::UnixStream::connect(p).is_ok() {
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("socket never appeared at {p:?}");
}

fn sigterm(child: &mut Child) -> std::process::ExitStatus {
    Command::new("kill")
        .args(["-TERM", &child.id().to_string()])
        .status()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(s) = child.try_wait().unwrap() {
            return s;
        }
        assert!(Instant::now() < deadline, "serve did not stop on SIGTERM");
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Drive a stdio MCP process: send lines, read one response per request.
fn converse(mut child: Child, requests: &[Value]) -> Vec<Value> {
    let mut stdin = child.stdin.take().unwrap();
    let mut out = BufReader::new(child.stdout.take().unwrap());
    let mut responses = Vec::new();
    for r in requests {
        writeln!(stdin, "{r}").unwrap();
        stdin.flush().unwrap();
        if r.get("id").is_some() {
            let mut line = String::new();
            out.read_line(&mut line).unwrap();
            responses.push(serde_json::from_str(&line).unwrap());
        }
    }
    drop(stdin);
    let _ = child.wait();
    responses
}

fn handshake() -> Vec<Value> {
    vec![
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"cli-test","version":"1"}}}),
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}),
        json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"resolve","arguments":{"handle":"🐙/w2-acme-2025"}}}),
    ]
}

#[test]
fn instructions_prints_the_server_instructions() {
    let s = Sandbox::new();
    s.cmd()
        .arg("instructions")
        .assert()
        .success()
        .stdout(format!("{}\n", inker_mcp::instructions()));
}

#[test]
fn serve_fake_hosts_the_socket_and_stops_on_sigterm() {
    let s = Sandbox::new();
    let mut child = s.serve(&[]);
    use std::os::unix::fs::PermissionsExt;
    assert_eq!(
        std::fs::metadata(s.socket()).unwrap().permissions().mode() & 0o777,
        0o600
    );
    let status = sigterm(&mut child);
    assert!(status.success(), "{status:?}");
    assert!(!s.socket().exists());
}

#[test]
fn mcp_shim_talks_to_the_host() {
    let s = Sandbox::new();
    let mut host = s.serve(&[]);
    let shim = s
        .std_cmd()
        .arg("mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let r = converse(shim, &handshake());
    assert_eq!(
        r[0]["result"]["instructions"],
        json!(inker_mcp::instructions())
    );
    assert_eq!(r[1]["result"]["tools"].as_array().unwrap().len(), 4);
    assert!(
        r[2]["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("[SSN_1]")
    );
    sigterm(&mut host);
}

#[test]
fn mcp_shim_without_app_prints_the_error_and_fails() {
    let s = Sandbox::new();
    let out = s
        .cmd()
        .arg("mcp")
        .write_stdin("")
        .assert()
        .failure()
        .get_output()
        .stdout
        .clone();
    let v: Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["error"]["code"], json!(-32000));
    assert_eq!(
        v["error"]["message"],
        json!("INKER is not running. Open INKER from your menu bar and try again.")
    );
}

#[test]
fn mcp_logs_never_touch_stdout() {
    let s = Sandbox::new();
    let out = s
        .cmd()
        .env("INKER_LOG", "trace")
        .arg("mcp")
        .write_stdin("")
        .assert()
        .failure()
        .get_output()
        .clone();
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert_eq!(
        stdout.lines().count(),
        1,
        "only the JSON-RPC line: {stdout}"
    );
}

#[test]
fn serve_stdio_speaks_mcp_directly() {
    let s = Sandbox::new();
    let child = s
        .std_cmd()
        .args(["serve", "--fake", "--stdio"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let r = converse(child, &handshake());
    assert_eq!(r[0]["result"]["serverInfo"]["name"], json!("inker"));
    assert!(
        r[2]["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("58,250.00")
    );
}

#[test]
fn serve_no_always_load_drops_the_meta() {
    let s = Sandbox::new();
    let child = s
        .std_cmd()
        .args(["serve", "--fake", "--stdio", "--no-always-load"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let r = converse(child, &handshake());
    let resolve = r[1]["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == json!("resolve"))
        .unwrap()
        .clone();
    assert!(resolve.get("_meta").is_none());
}

#[test]
fn list_resolve_describe_over_the_socket() {
    let s = Sandbox::new();
    let mut host = s.serve(&[]);
    s.cmd().arg("list").assert().success().stdout(
        predicate::str::contains("🐙/w2-acme-2025").and(predicate::str::contains("🐙/taxes-2025")),
    );
    s.cmd()
        .args(["resolve", "ink/w2-acme-2025"])
        .assert()
        .success()
        .stdout(predicate::str::contains("[EIN_1]"));
    s.cmd()
        .args(["describe", "w2-acme-2025"])
        .assert()
        .success()
        .stdout(predicate::str::contains("[SSN_1]: employee"));
    s.cmd()
        .args(["resolve", "🐙/1099-int-bank-2025"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "pending review: the user has not finished reviewing 🐙/1099-int-bank-2025",
        ));
    let out = s
        .cmd()
        .args(["list", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["entries"].as_array().unwrap().len(), 4);
    sigterm(&mut host);
}

#[test]
fn client_commands_without_app_fail_clearly() {
    let s = Sandbox::new();
    s.cmd()
        .arg("list")
        .assert()
        .failure()
        .stderr(predicate::str::contains("INKER is not running"));
}

#[test]
fn setup_dry_run_shows_the_plan_and_writes_nothing() {
    let s = Sandbox::new();
    s.cmd()
        .args(["setup", "--agent", "all", "--dry-run"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains(".claude/settings.json")
                .and(predicate::str::contains(".cursor/mcp.json"))
                .and(predicate::str::contains(".codex/config.toml"))
                .and(predicate::str::contains("Read(~/Private/**)")),
        );
    let entries: Vec<_> = std::fs::read_dir(s.home()).unwrap().collect();
    assert!(entries.is_empty(), "dry run wrote {entries:?}");
}

#[test]
fn setup_writes_into_home_with_a_custom_private_folder() {
    let s = Sandbox::new();
    s.cmd()
        .args(["setup", "--agent", "codex", "--private", "~/Vault"])
        .assert()
        .success();
    let cfg = std::fs::read_to_string(s.home().join(".codex/config.toml")).unwrap();
    assert!(cfg.contains("[mcp_servers.inker]"));
    assert!(!cfg.contains("default_permissions"), "profile is opt-in");
    let hooks = std::fs::read_to_string(s.home().join(".codex/hooks.json")).unwrap();
    assert!(
        hooks.contains(&format!("--path {}", s.home().join("Vault").display())),
        "{hooks}"
    );
    s.cmd()
        .args([
            "setup",
            "--agent",
            "codex",
            "--private",
            "~/Vault",
            "--codex-permission-profile",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("sandbox_mode"));
    assert!(
        std::fs::read_to_string(s.home().join(".codex/config.toml"))
            .unwrap()
            .contains("default_permissions = \"inker\"")
    );
}

#[test]
fn setup_claude_writes_settings() {
    let s = Sandbox::new();
    s.cmd()
        .args(["setup", "--agent", "claude"])
        .assert()
        .success();
    let settings = std::fs::read_to_string(s.home().join(".claude/settings.json")).unwrap();
    assert!(settings.contains("Read(~/Private/**)"));
    assert!(settings.contains(" guard --path "));
}

#[test]
fn teach_writes_agents_md_in_cwd() {
    let s = Sandbox::new();
    let proj = s.dir.path().join("proj");
    std::fs::create_dir_all(&proj).unwrap();
    s.cmd()
        .arg("teach")
        .arg("--dry-run")
        .current_dir(&proj)
        .assert()
        .success()
        .stdout(predicate::str::contains("AGENTS.md"));
    assert!(!proj.join("AGENTS.md").exists());
    s.cmd().arg("teach").current_dir(&proj).assert().success();
    assert!(
        std::fs::read_to_string(proj.join("AGENTS.md"))
            .unwrap()
            .contains(inker_agents::TEACHING_LINE)
    );
}

#[test]
fn doctor_reports_each_agent_and_the_socket() {
    let s = Sandbox::new();
    s.cmd()
        .args(["setup", "--agent", "cursor"])
        .assert()
        .success();
    s.cmd().arg("doctor").assert().success().stdout(
        predicate::str::contains("Claude Code: not installed")
            .and(predicate::str::contains("Cursor: installed, registered"))
            .and(predicate::str::contains("beforeReadFile hook: yes"))
            .and(predicate::str::contains("MCP: not running")),
    );
    let mut host = s.serve(&[]);
    s.cmd()
        .arg("doctor")
        .assert()
        .success()
        .stdout(predicate::str::contains("MCP: running"));
    sigterm(&mut host);
}

#[test]
fn guard_allows_and_denies() {
    let s = Sandbox::new();
    let private = s.home().join("Private");
    let p = private.to_str().unwrap();
    s.cmd()
        .args(["guard", "--path", p])
        .write_stdin(
            json!({"tool_name":"Read","tool_input":{"file_path":"/etc/hosts"}}).to_string(),
        )
        .assert()
        .code(0)
        .stdout("");
    s.cmd()
        .args(["guard", "--path", p])
        .write_stdin(
            json!({"tool_name":"Read","tool_input":{"file_path": format!("{p}/w2.pdf")}})
                .to_string(),
        )
        .assert()
        .code(2)
        .stderr(predicate::str::contains("resolve"));
    s.cmd()
        .args(["guard", "--format", "cursor", "--path", p])
        .write_stdin(
            json!({"hook_event_name":"beforeShellExecution","command":"cat ~/Private/a"})
                .to_string(),
        )
        .assert()
        .code(2)
        .stdout(predicate::str::contains("\"permission\":\"deny\""));
    s.cmd()
        .args(["guard", "--format", "cursor", "--path", p])
        .write_stdin(json!({"hook_event_name":"beforeReadFile","file_path":"/tmp/x"}).to_string())
        .assert()
        .code(0)
        .stdout("{\"permission\":\"allow\"}\n");
}

#[test]
fn logs_to_a_pipe_have_no_ansi_escapes() {
    let s = Sandbox::new();
    let mut child = s
        .std_cmd()
        .env("INKER_LOG", "inker_mcp=debug")
        .args(["serve", "--fake", "--stdio"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let stderr = child.stderr.take().unwrap();
    let _ = converse(child, &handshake());
    let mut log = String::new();
    BufReader::new(stderr).read_to_string(&mut log).unwrap();
    assert!(log.contains("tool=resolve"), "{log}");
    assert!(!log.contains('\u{1b}'), "{log:?}");
}

const SAMPLES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../samples/synthetic");

impl Sandbox {
    fn private(&self) -> PathBuf {
        self.dir.path().join("Private")
    }
    /// `inker serve --private <sandbox>` with the real core, waiting for the socket.
    fn serve_real(&self) -> Child {
        let child = self
            .std_cmd()
            .args(["serve", "--private"])
            .arg(self.private())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        wait_for_socket(&self.socket());
        child
    }
    fn download(&self, name: &str) -> PathBuf {
        let downloads = self.dir.path().join("Downloads");
        std::fs::create_dir_all(&downloads).unwrap();
        let dst = downloads.join(name);
        std::fs::copy(Path::new(SAMPLES).join(name), &dst).unwrap();
        dst
    }
}

#[test]
fn ingest_moves_a_file_and_prints_its_handle() {
    let s = Sandbox::new();
    let src = s.download("w2-acme-2025.txt");
    let out = s
        .cmd()
        .args(["ingest", "--private"])
        .arg(s.private())
        .arg(&src)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let out = String::from_utf8(out).unwrap();
    assert!(out.contains("🐙/w2-"), "{out}");
    assert!(out.contains("ready"), "{out}");
    assert!(
        !out.contains("900-12-3456"),
        "a real value reached stdout: {out}"
    );
    assert!(
        !src.exists(),
        "the original should have moved out of Downloads"
    );
    assert!(s.private().join("w2-acme-2025.txt").exists());
}

#[test]
fn ingest_copy_keeps_the_original() {
    let s = Sandbox::new();
    let src = s.download("1099-int-harbor-2025.txt");
    s.cmd()
        .args(["ingest", "--copy", "--private"])
        .arg(s.private())
        .arg(&src)
        .assert()
        .success()
        .stdout(predicate::str::contains("🐙/"));
    assert!(src.exists());
}

#[test]
fn serve_real_core_lists_and_resolves_an_ingested_document() {
    let s = Sandbox::new();
    let src = s.download("w2-acme-2025.txt");
    s.cmd()
        .args(["ingest", "--private"])
        .arg(s.private())
        .arg(&src)
        .assert()
        .success();

    let mut host = s.serve_real();
    let list = s
        .cmd()
        .args(["list", "--socket"])
        .arg(s.socket())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let list = String::from_utf8(list).unwrap();
    let handle = list
        .split_whitespace()
        .find(|w| w.starts_with("🐙/w2-"))
        .unwrap_or_else(|| panic!("no W-2 handle in list output: {list}"))
        .to_string();

    let text = s
        .cmd()
        .args(["resolve", "--socket"])
        .arg(s.socket())
        .arg(&handle)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(text).unwrap();
    assert!(text.contains("[SSN_"), "{text}");
    assert!(
        !text.contains("900-12-3456"),
        "real value over the socket: {text}"
    );

    host.kill().unwrap();
    host.wait().unwrap();
}

#[test]
fn serve_real_core_refuses_when_the_private_folder_is_not_utf8_or_missing_parent() {
    let s = Sandbox::new();
    // A private folder under a file cannot be created.
    let blocker = s.dir.path().join("blocker");
    std::fs::write(&blocker, b"x").unwrap();
    s.cmd()
        .args(["serve", "--private"])
        .arg(blocker.join("Private"))
        .assert()
        .failure();
}

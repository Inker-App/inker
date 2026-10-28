#![allow(clippy::unwrap_used, clippy::expect_used)]
mod common;

use common::TestHome;
use inker_agents::{DryRun, cursor};
use serde_json::json;

#[test]
fn registers_in_mcp_json_and_adds_both_hooks() {
    let t = TestHome::new();
    let plan = cursor::setup(&t.env, DryRun(false)).unwrap();
    assert_eq!(
        t.json(".cursor/mcp.json")["mcpServers"]["inker"],
        json!({"command": "/opt/inker/bin/inker", "args": ["mcp"]})
    );
    let h = t.json(".cursor/hooks.json");
    assert_eq!(h["version"], json!(1));
    for event in ["beforeReadFile", "beforeShellExecution"] {
        let cmd = h["hooks"][event][0]["command"].as_str().unwrap();
        assert!(
            cmd.starts_with("/opt/inker/bin/inker guard --format cursor --path "),
            "{cmd}"
        );
    }
    assert!(
        plan.warnings.iter().any(|w| w.contains("no deny rule")),
        "{:?}",
        plan.warnings
    );
    assert!(
        plan.warnings.iter().any(|w| w.contains("inker teach")),
        "{:?}",
        plan.warnings
    );
}

#[test]
fn keeps_other_servers_and_hooks() {
    let t = TestHome::new();
    t.write(
        ".cursor/mcp.json",
        r#"{"mcpServers": {"github": {"url": "https://example.invalid/mcp"}}}"#,
    );
    t.write(".cursor/hooks.json", r#"{"version": 1, "hooks": {"afterFileEdit": [{"command": "./fmt.sh"}], "beforeReadFile": [{"command": "./audit.sh"}]}}"#);
    cursor::setup(&t.env, DryRun(false)).unwrap();
    let m = t.json(".cursor/mcp.json");
    assert_eq!(
        m["mcpServers"]["github"]["url"],
        json!("https://example.invalid/mcp")
    );
    let h = t.json(".cursor/hooks.json");
    assert_eq!(
        h["hooks"]["afterFileEdit"],
        json!([{"command": "./fmt.sh"}])
    );
    assert_eq!(h["hooks"]["beforeReadFile"].as_array().unwrap().len(), 2);
}

#[test]
fn idempotent() {
    let t = TestHome::new();
    cursor::setup(&t.env, DryRun(false)).unwrap();
    let plan = cursor::setup(&t.env, DryRun(false)).unwrap();
    assert!(
        plan.edits.is_empty() && plan.commands.is_empty(),
        "{plan:?}"
    );
}

#[test]
fn does_not_write_while_cursor_runs() {
    let mut t = TestHome::new();
    t.env.cursor_running = true;
    let plan = cursor::setup(&t.env, DryRun(false)).unwrap();
    assert!(plan.edits.is_empty());
    assert!(!t.path(".cursor/mcp.json").exists());
    assert!(
        plan.warnings.iter().any(|w| w.contains("Quit Cursor")),
        "{:?}",
        plan.warnings
    );
}

#[test]
fn dry_run_writes_nothing() {
    let t = TestHome::new();
    let plan = cursor::setup(&t.env, DryRun(true)).unwrap();
    assert_eq!(plan.edits.len(), 2);
    assert!(plan.edits.iter().all(|e| e.before.is_none()));
    assert!(!t.path(".cursor").exists());
}

#![allow(clippy::unwrap_used, clippy::expect_used)]
mod common;

use common::TestHome;
use inker_agents::{DryRun, Error, claude_desktop};
use serde_json::json;

const CONFIG: &str = "Library/Application Support/Claude/claude_desktop_config.json";

const FRESH: &str = r#"{
  "mcpServers": {
    "inker": {
      "command": "/opt/inker/bin/inker",
      "args": [
        "mcp"
      ],
      "env": {}
    }
  }
}
"#;

#[test]
fn config_path_is_under_application_support() {
    let t = TestHome::new();
    assert_eq!(claude_desktop::config_path(&t.env), t.path(CONFIG));
}

#[test]
fn missing_file_creates_directory_and_file() {
    let t = TestHome::new();
    let plan = claude_desktop::setup(&t.env, DryRun(false)).unwrap();
    assert_eq!(plan.edits.len(), 1);
    assert!(plan.edits[0].before.is_none());
    assert!(plan.commands.is_empty());
    assert_eq!(t.read(CONFIG), FRESH);
}

#[test]
fn warning_says_to_quit_and_reopen_claude() {
    let t = TestHome::new();
    let plan = claude_desktop::setup(&t.env, DryRun(true)).unwrap();
    assert!(
        plan.warnings
            .iter()
            .any(|w| w.contains("Quit Claude") && w.contains("open it again")),
        "{:?}",
        plan.warnings
    );
}

#[test]
fn empty_file_is_treated_as_empty_config() {
    let t = TestHome::new();
    t.write(CONFIG, "  \n");
    let plan = claude_desktop::setup(&t.env, DryRun(false)).unwrap();
    assert_eq!(plan.edits[0].before.as_deref(), Some("  \n"));
    assert_eq!(t.read(CONFIG), FRESH);
}

#[test]
fn keeps_other_servers_and_keys_in_order() {
    let t = TestHome::new();
    t.write(
        CONFIG,
        r#"{"globalShortcut": "Alt+Space", "mcpServers": {"notes": {"command": "/usr/local/bin/notes-mcp", "args": ["--stdio"], "env": {"NOTES_DIR": "/tmp/notes"}}}, "zeta": [1, 2]}"#,
    );
    claude_desktop::setup(&t.env, DryRun(false)).unwrap();
    let text = t.read(CONFIG);
    let v = t.json(CONFIG);
    assert_eq!(v["globalShortcut"], json!("Alt+Space"));
    assert_eq!(v["zeta"], json!([1, 2]));
    assert_eq!(
        v["mcpServers"]["notes"],
        json!({"command": "/usr/local/bin/notes-mcp", "args": ["--stdio"], "env": {"NOTES_DIR": "/tmp/notes"}})
    );
    assert_eq!(
        v["mcpServers"]["inker"],
        json!({"command": "/opt/inker/bin/inker", "args": ["mcp"], "env": {}})
    );
    let pos = |k: &str| text.find(k).unwrap();
    assert!(pos("globalShortcut") < pos("mcpServers"));
    assert!(pos("mcpServers") < pos("zeta"));
    assert!(pos("\"notes\"") < pos("\"inker\""));
}

#[test]
fn second_run_is_an_empty_plan_and_leaves_bytes_alone() {
    let t = TestHome::new();
    claude_desktop::setup(&t.env, DryRun(false)).unwrap();
    let plan = claude_desktop::setup(&t.env, DryRun(false)).unwrap();
    assert!(plan.is_empty(), "{plan:?}");
    assert_eq!(t.read(CONFIG), FRESH);
}

#[test]
fn identical_entry_in_compact_form_is_left_alone() {
    let t = TestHome::new();
    let compact =
        r#"{"mcpServers":{"inker":{"command":"/opt/inker/bin/inker","args":["mcp"],"env":{}}}}"#;
    t.write(CONFIG, compact);
    let plan = claude_desktop::setup(&t.env, DryRun(false)).unwrap();
    assert!(plan.is_empty(), "{plan:?}");
    assert_eq!(t.read(CONFIG), compact);
}

#[test]
fn stale_entry_is_replaced() {
    let t = TestHome::new();
    t.write(
        CONFIG,
        r#"{"mcpServers": {"inker": {"command": "/old/inker", "args": ["mcp"]}}}"#,
    );
    let plan = claude_desktop::setup(&t.env, DryRun(false)).unwrap();
    assert_eq!(plan.edits.len(), 1);
    assert_eq!(t.read(CONFIG), FRESH);
}

#[test]
fn malformed_json_refuses_and_writes_nothing() {
    let t = TestHome::new();
    t.write(CONFIG, "{\"mcpServers\": {");
    let err = claude_desktop::setup(&t.env, DryRun(false)).unwrap_err();
    assert!(matches!(err, Error::Parse { .. }), "{err:?}");
    let msg = err.to_string();
    assert!(msg.contains("claude_desktop_config.json"), "{msg}");
    assert!(msg.contains("fix or remove"), "{msg}");
    assert_eq!(t.read(CONFIG), "{\"mcpServers\": {");
}

#[test]
fn non_object_root_or_servers_refuses() {
    let t = TestHome::new();
    for bad in ["[1, 2]", r#"{"mcpServers": ["x"]}"#] {
        t.write(CONFIG, bad);
        let err = claude_desktop::setup(&t.env, DryRun(false)).unwrap_err();
        assert!(matches!(err, Error::Parse { .. }), "{bad}: {err:?}");
        assert_eq!(t.read(CONFIG), bad);
    }
}

#[test]
fn dry_run_writes_nothing() {
    let t = TestHome::new();
    let plan = claude_desktop::setup(&t.env, DryRun(true)).unwrap();
    assert_eq!(plan.edits.len(), 1);
    assert_eq!(plan.edits[0].after, FRESH);
    assert!(!t.path("Library").exists());
}

#[test]
fn installed_when_config_dir_or_app_exists() {
    let mut t = TestHome::new();
    assert!(!claude_desktop::installed(&t.env));
    let apps = t.dir.path().join("Applications");
    std::fs::create_dir_all(apps.join("Claude.app")).unwrap();
    t.env.applications = apps;
    assert!(claude_desktop::installed(&t.env));
    t.env.applications = t.dir.path().join("nowhere");
    assert!(!claude_desktop::installed(&t.env));
    std::fs::create_dir_all(t.path("Library/Application Support/Claude")).unwrap();
    assert!(claude_desktop::installed(&t.env));
}

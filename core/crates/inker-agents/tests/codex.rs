#![allow(clippy::unwrap_used, clippy::expect_used)]
mod common;

use common::TestHome;
use inker_agents::DryRun;
use inker_agents::codex::{self, CodexOptions, PROFILE_WARNING};
use serde_json::json;

fn opted_in() -> CodexOptions {
    CodexOptions {
        permission_profile: true,
    }
}

#[test]
fn registers_server_hook_and_agents_line_without_profile_by_default() {
    let t = TestHome::new();
    let plan = codex::setup(&t.env, CodexOptions::default(), DryRun(false)).unwrap();
    let cfg = t.read(".codex/config.toml");
    let doc: toml_edit::DocumentMut = cfg.parse().unwrap();
    assert_eq!(
        doc["mcp_servers"]["inker"]["command"].as_str(),
        Some("/opt/inker/bin/inker")
    );
    assert_eq!(
        doc["mcp_servers"]["inker"]["args"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect::<Vec<_>>(),
        ["mcp"]
    );
    assert!(doc.get("default_permissions").is_none());
    assert!(doc.get("permissions").is_none());
    let hooks = t.json(".codex/hooks.json");
    let cmd = hooks["hooks"]["PreToolUse"][0]["hooks"][0]["command"]
        .as_str()
        .unwrap();
    assert!(
        cmd.starts_with("/opt/inker/bin/inker guard --path "),
        "{cmd}"
    );
    assert!(
        t.read(".codex/AGENTS.md")
            .contains(inker_agents::TEACHING_LINE)
    );
    assert!(!plan.warnings.contains(&PROFILE_WARNING.to_string()));
}

#[test]
fn profile_only_on_opt_in_with_the_warning() {
    let t = TestHome::new();
    let plan = codex::setup(&t.env, opted_in(), DryRun(false)).unwrap();
    assert!(plan.warnings.contains(&PROFILE_WARNING.to_string()));
    assert!(PROFILE_WARNING.contains("sandbox_mode"));
    let doc: toml_edit::DocumentMut = t.read(".codex/config.toml").parse().unwrap();
    assert_eq!(doc["default_permissions"].as_str(), Some("inker"));
    let private = t.home().join("Private").display().to_string();
    assert_eq!(
        doc["permissions"]["inker"]["filesystem"][private.as_str()].as_str(),
        Some("deny")
    );
}

#[test]
fn preserves_formatting_and_comments() {
    let t = TestHome::new();
    let before = "# my codex config\nmodel = \"gpt-5\"   # keep this comment\n\n[mcp_servers.other]\ncommand = \"other\"\n";
    t.write(".codex/config.toml", before);
    codex::setup(&t.env, opted_in(), DryRun(false)).unwrap();
    let after = t.read(".codex/config.toml");
    assert!(
        after.starts_with("# my codex config\nmodel = \"gpt-5\"   # keep this comment\n"),
        "{after}"
    );
    assert!(after.contains("[mcp_servers.other]\ncommand = \"other\"\n"));
    assert!(after.contains("[mcp_servers.inker]"));
    // default_permissions is a top-level key, so it must land before the first table.
    let doc: toml_edit::DocumentMut = after.parse().unwrap();
    assert_eq!(doc["default_permissions"].as_str(), Some("inker"));
    assert_eq!(doc["model"].as_str(), Some("gpt-5"));
}

#[test]
fn never_overrides_an_existing_default_profile() {
    let t = TestHome::new();
    t.write(".codex/config.toml", "default_permissions = \"work\"\n");
    let plan = codex::setup(&t.env, opted_in(), DryRun(false)).unwrap();
    let doc: toml_edit::DocumentMut = t.read(".codex/config.toml").parse().unwrap();
    assert_eq!(doc["default_permissions"].as_str(), Some("work"));
    assert!(
        plan.warnings.iter().any(|w| w.contains("\"work\"")),
        "{:?}",
        plan.warnings
    );
}

#[test]
fn idempotent() {
    let t = TestHome::new();
    codex::setup(&t.env, opted_in(), DryRun(false)).unwrap();
    let before = t.read(".codex/config.toml");
    let plan = codex::setup(&t.env, opted_in(), DryRun(false)).unwrap();
    assert!(plan.edits.is_empty(), "{:#?}", plan.edits);
    assert_eq!(before, t.read(".codex/config.toml"));
}

#[test]
fn keeps_existing_hooks() {
    let t = TestHome::new();
    t.write(".codex/hooks.json", r#"{"hooks": {"PreToolUse": [{"matcher": "shell", "hooks": [{"type": "command", "command": "audit"}]}], "Stop": []}}"#);
    codex::setup(&t.env, CodexOptions::default(), DryRun(false)).unwrap();
    let h = t.json(".codex/hooks.json");
    assert_eq!(h["hooks"]["Stop"], json!([]));
    assert_eq!(h["hooks"]["PreToolUse"].as_array().unwrap().len(), 2);
}

#[test]
fn malformed_toml_is_not_clobbered() {
    let t = TestHome::new();
    t.write(".codex/config.toml", "[[[ nope");
    assert!(codex::setup(&t.env, CodexOptions::default(), DryRun(false)).is_err());
    assert_eq!(t.read(".codex/config.toml"), "[[[ nope");
}

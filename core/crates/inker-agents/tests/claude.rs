#![allow(clippy::unwrap_used, clippy::expect_used)]
mod common;

use common::TestHome;
use inker_agents::{DryRun, claude};
use serde_json::json;

const GUARD: &str = "/opt/inker/bin/inker guard --path";

#[test]
fn fresh_home_without_claude_binary_edits_claude_json() {
    let t = TestHome::new();
    let plan = claude::setup(&t.env, DryRun(false)).unwrap();
    assert!(plan.commands.is_empty());
    let cj = t.json(".claude.json");
    assert_eq!(
        cj["mcpServers"]["inker"],
        json!({"type": "stdio", "command": "/opt/inker/bin/inker", "args": ["mcp"], "env": {}})
    );
}

#[test]
fn settings_get_deny_rules_sandbox_and_hook() {
    let t = TestHome::new();
    claude::setup(&t.env, DryRun(false)).unwrap();
    let s = t.json(".claude/settings.json");
    assert_eq!(
        s["permissions"]["deny"],
        json!(["Read(~/Private/**)", "Edit(~/Private/**)"])
    );
    assert_eq!(s["sandbox"]["filesystem"]["denyRead"], json!(["~/Private"]));
    let pre = s["hooks"]["PreToolUse"].as_array().unwrap();
    assert_eq!(pre.len(), 1);
    let cmd = pre[0]["hooks"][0]["command"].as_str().unwrap();
    assert!(cmd.starts_with(GUARD), "{cmd}");
    assert!(cmd.ends_with("/home/Private"), "{cmd}");
    assert_eq!(pre[0]["hooks"][0]["type"], json!("command"));
    assert!(pre[0]["matcher"].as_str().unwrap().contains("Read"));
}

#[test]
fn merge_keeps_other_keys_and_their_order() {
    let t = TestHome::new();
    let before = r#"{
  "model": "opus",
  "permissions": {
    "allow": ["Bash(git status)"],
    "deny": ["Read(./.env)"],
    "defaultMode": "acceptEdits"
  },
  "hooks": {
    "PreToolUse": [{"matcher": "Bash", "hooks": [{"type": "command", "command": "echo hi"}]}]
  },
  "zeta": 1
}"#;
    t.write(".claude/settings.json", before);
    claude::setup(&t.env, DryRun(false)).unwrap();
    let after = t.read(".claude/settings.json");
    let s: serde_json::Value = serde_json::from_str(&after).unwrap();
    assert_eq!(s["model"], json!("opus"));
    assert_eq!(s["zeta"], json!(1));
    assert_eq!(s["permissions"]["allow"], json!(["Bash(git status)"]));
    assert_eq!(s["permissions"]["defaultMode"], json!("acceptEdits"));
    assert_eq!(
        s["permissions"]["deny"],
        json!(["Read(./.env)", "Read(~/Private/**)", "Edit(~/Private/**)"])
    );
    assert_eq!(s["hooks"]["PreToolUse"].as_array().unwrap().len(), 2);
    // Order: model < permissions < hooks < zeta, as the user wrote it.
    let pos = |k: &str| after.find(&format!("\"{k}\"")).unwrap();
    assert!(
        pos("model") < pos("permissions")
            && pos("permissions") < pos("hooks")
            && pos("hooks") < pos("zeta")
    );
    assert!(pos("allow") < pos("deny") && pos("deny") < pos("defaultMode"));
}

#[test]
fn second_run_is_a_no_op() {
    let t = TestHome::new();
    claude::setup(&t.env, DryRun(false)).unwrap();
    let snapshot = (
        t.read(".claude/settings.json"),
        t.read(".claude.json"),
        t.read(".claude/CLAUDE.md"),
    );
    let plan = claude::setup(&t.env, DryRun(false)).unwrap();
    assert!(plan.is_empty(), "{plan:?}");
    assert_eq!(
        snapshot,
        (
            t.read(".claude/settings.json"),
            t.read(".claude.json"),
            t.read(".claude/CLAUDE.md")
        )
    );
}

#[test]
fn dry_run_writes_nothing_and_shows_before_after() {
    let t = TestHome::new();
    t.write(".claude/settings.json", "{\"model\": \"opus\"}\n");
    let plan = claude::setup(&t.env, DryRun(true)).unwrap();
    assert_eq!(t.read(".claude/settings.json"), "{\"model\": \"opus\"}\n");
    assert!(!t.path(".claude.json").exists());
    assert!(!t.path(".claude/CLAUDE.md").exists());
    let e = plan
        .edits
        .iter()
        .find(|e| e.path.ends_with(".claude/settings.json"))
        .unwrap();
    assert_eq!(e.before.as_deref(), Some("{\"model\": \"opus\"}\n"));
    assert!(e.after.contains("Read(~/Private/**)"));
    assert!(!e.summary.is_empty());
    assert_eq!(
        plan.edits.len(),
        3,
        "settings, ~/.claude.json, CLAUDE.md: {plan:#?}"
    );
}

#[test]
fn uses_claude_mcp_add_when_the_binary_is_on_path() {
    let mut t = TestHome::new();
    let bin = t.fake_bin("claude");
    let plan = claude::setup(&t.env, DryRun(false)).unwrap();
    assert_eq!(plan.commands.len(), 1);
    assert_eq!(plan.commands[0].program, bin);
    assert_eq!(
        plan.commands[0].args,
        [
            "mcp",
            "add",
            "--scope",
            "user",
            "inker",
            "--",
            "/opt/inker/bin/inker",
            "mcp"
        ]
    );
    assert_eq!(
        t.fake_log("claude").trim(),
        "mcp add --scope user inker -- /opt/inker/bin/inker mcp"
    );
    assert!(
        !t.path(".claude.json").exists(),
        "the CLI owns ~/.claude.json"
    );
}

#[test]
fn already_registered_needs_no_command() {
    let mut t = TestHome::new();
    t.fake_bin("claude");
    t.write(".claude.json", r#"{"mcpServers":{"inker":{"type":"stdio","command":"/opt/inker/bin/inker","args":["mcp"],"env":{}}}}"#);
    let plan = claude::setup(&t.env, DryRun(true)).unwrap();
    assert!(plan.commands.is_empty());
}

#[test]
fn stale_registration_is_replaced_through_the_cli() {
    let mut t = TestHome::new();
    t.fake_bin("claude");
    t.write(
        ".claude.json",
        r#"{"mcpServers":{"inker":{"type":"stdio","command":"/old/inker","args":["mcp"]}}}"#,
    );
    let plan = claude::setup(&t.env, DryRun(true)).unwrap();
    let lines: Vec<String> = plan.commands.iter().map(|c| c.args.join(" ")).collect();
    assert_eq!(
        lines,
        [
            "mcp remove --scope user inker",
            "mcp add --scope user inker -- /opt/inker/bin/inker mcp"
        ]
    );
}

#[test]
fn running_claude_without_cli_leaves_claude_json_alone() {
    let mut t = TestHome::new();
    t.env.claude_running = true;
    t.write(".claude.json", "{\"numStartups\": 3}");
    let plan = claude::setup(&t.env, DryRun(false)).unwrap();
    assert_eq!(t.read(".claude.json"), "{\"numStartups\": 3}");
    assert!(
        plan.warnings
            .iter()
            .any(|w| w.contains("claude mcp add --scope user inker")),
        "{:?}",
        plan.warnings
    );
    // The rest still happens.
    assert!(
        t.read(".claude/settings.json")
            .contains("Read(~/Private/**)")
    );
}

#[test]
fn claude_json_edit_keeps_everything_else() {
    let t = TestHome::new();
    t.write(".claude.json", r#"{"numStartups": 3, "projects": {"/x": {"mcpServers": {}}}, "mcpServers": {"other": {"command": "o"}}}"#);
    claude::setup(&t.env, DryRun(false)).unwrap();
    let cj = t.json(".claude.json");
    assert_eq!(cj["numStartups"], json!(3));
    assert_eq!(cj["projects"]["/x"], json!({"mcpServers": {}}));
    assert_eq!(cj["mcpServers"]["other"], json!({"command": "o"}));
    assert!(cj["mcpServers"]["inker"].is_object());
}

#[test]
fn private_folder_outside_home_uses_absolute_rule_syntax() {
    let mut t = TestHome::new();
    t.env.private = "/Volumes/Vault/Private".into();
    claude::setup(&t.env, DryRun(false)).unwrap();
    let s = t.json(".claude/settings.json");
    assert_eq!(
        s["permissions"]["deny"],
        json!([
            "Read(//Volumes/Vault/Private/**)",
            "Edit(//Volumes/Vault/Private/**)"
        ])
    );
    assert_eq!(
        s["sandbox"]["filesystem"]["denyRead"],
        json!(["/Volumes/Vault/Private"])
    );
}

#[test]
fn paths_with_spaces_are_quoted_in_the_hook() {
    let mut t = TestHome::new();
    t.env.private = t.home().join("My Private");
    claude::setup(&t.env, DryRun(false)).unwrap();
    let s = t.json(".claude/settings.json");
    let cmd = s["hooks"]["PreToolUse"][0]["hooks"][0]["command"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(cmd.ends_with("/home/My Private'"), "{cmd}");
}

#[test]
fn claude_md_gets_the_teaching_block_once() {
    let t = TestHome::new();
    t.write(".claude/CLAUDE.md", "# Mine\n\nKeep this.\n");
    claude::setup(&t.env, DryRun(false)).unwrap();
    claude::setup(&t.env, DryRun(false)).unwrap();
    let md = t.read(".claude/CLAUDE.md");
    assert!(md.starts_with("# Mine\n\nKeep this.\n"));
    assert_eq!(md.matches(inker_agents::TEACH_START).count(), 1);
    assert!(md.contains(inker_agents::TEACHING_LINE));
}

#[test]
fn warns_when_sandbox_is_not_enabled() {
    let t = TestHome::new();
    let plan = claude::setup(&t.env, DryRun(true)).unwrap();
    assert!(
        plan.warnings.iter().any(|w| w.contains("sandbox")),
        "{:?}",
        plan.warnings
    );
    t.write(".claude/settings.json", r#"{"sandbox": {"enabled": true}}"#);
    let plan = claude::setup(&t.env, DryRun(true)).unwrap();
    assert!(
        !plan.warnings.iter().any(|w| w.contains("sandbox")),
        "{:?}",
        plan.warnings
    );
}

#[test]
fn malformed_settings_are_not_clobbered() {
    let t = TestHome::new();
    t.write(".claude/settings.json", "{ not json");
    let err = claude::setup(&t.env, DryRun(false)).unwrap_err();
    assert!(matches!(err, inker_agents::Error::Parse { .. }), "{err}");
    assert_eq!(t.read(".claude/settings.json"), "{ not json");
}

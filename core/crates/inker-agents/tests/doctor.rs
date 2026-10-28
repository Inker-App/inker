#![allow(clippy::unwrap_used, clippy::expect_used)]
mod common;

use common::TestHome;
use inker_agents::codex::CodexOptions;
use inker_agents::{Agent, AgentStatus, DryRun, claude, codex, cursor, doctor};

fn status(all: &[AgentStatus], a: Agent) -> AgentStatus {
    all.iter().find(|s| s.agent == a).unwrap().clone()
}

#[test]
fn empty_home_reports_nothing_installed() {
    let t = TestHome::new();
    let all = doctor(&t.env);
    assert_eq!(all.len(), 3);
    for s in &all {
        assert!(!s.installed && !s.registered, "{s:?}");
        assert!(s.layers.iter().all(|(_, on)| !on), "{s:?}");
    }
}

#[test]
fn config_dir_or_binary_counts_as_installed() {
    let mut t = TestHome::new();
    std::fs::create_dir_all(t.path(".cursor")).unwrap();
    t.fake_bin("codex");
    let all = doctor(&t.env);
    assert!(!status(&all, Agent::ClaudeCode).installed);
    assert!(status(&all, Agent::Cursor).installed);
    assert!(status(&all, Agent::Codex).installed);
}

#[test]
fn after_setup_everything_is_in_place() {
    let t = TestHome::new();
    claude::setup(&t.env, DryRun(false)).unwrap();
    cursor::setup(&t.env, DryRun(false)).unwrap();
    codex::setup(
        &t.env,
        CodexOptions {
            permission_profile: true,
        },
        DryRun(false),
    )
    .unwrap();
    for s in doctor(&t.env) {
        assert!(s.installed && s.registered, "{s:?}");
        assert!(s.layers.iter().all(|(_, on)| *on), "{s:?}");
    }
    let c = status(&doctor(&t.env), Agent::ClaudeCode);
    let names: Vec<&str> = c.layers.iter().map(|(n, _)| *n).collect();
    assert_eq!(
        names,
        [
            "permissions.deny",
            "sandbox.denyRead",
            "PreToolUse hook",
            "CLAUDE.md line"
        ]
    );
}

#[test]
fn codex_without_opt_in_shows_profile_missing() {
    let t = TestHome::new();
    codex::setup(&t.env, CodexOptions::default(), DryRun(false)).unwrap();
    let c = status(&doctor(&t.env), Agent::Codex);
    assert!(c.registered);
    assert_eq!(
        c.layers,
        vec![
            ("permission profile", false),
            ("PreToolUse hook", true),
            ("AGENTS.md line", true)
        ]
    );
}

#[test]
fn a_different_private_folder_is_not_covered() {
    let mut t = TestHome::new();
    claude::setup(&t.env, DryRun(false)).unwrap();
    t.env.private = t.home().join("Elsewhere");
    let c = status(&doctor(&t.env), Agent::ClaudeCode);
    assert!(c.registered);
    assert_eq!(
        c.layers.iter().filter(|(_, on)| *on).count(),
        1,
        "only the CLAUDE.md line: {c:?}"
    );
}

//! `inker doctor`: what is installed and configured, per agent.

use std::path::Path;

use crate::codex::PROFILE;
use crate::ojson::{self, J};
use crate::plan::read_optional;
use crate::teach::{TEACH_START, TEACHING_LINE};
use crate::{Agent, Env, claude, codex, cursor};

/// One agent's status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentStatus {
    pub agent: Agent,
    pub installed: bool,
    pub registered: bool,
    /// Exclusion and teaching layers, by name, and whether each is in place.
    pub layers: Vec<(&'static str, bool)>,
}

fn json(path: &Path) -> J {
    read_optional(path)
        .ok()
        .flatten()
        .and_then(|t| ojson::parse(&t).ok())
        .unwrap_or(J::Null)
}

fn has_block(path: &Path) -> bool {
    read_optional(path)
        .ok()
        .flatten()
        .is_some_and(|t| t.contains(TEACH_START) && t.contains(TEACHING_LINE))
}

fn installed(env: &Env, bin: &str, dir: &str) -> bool {
    env.which(bin).is_some() || env.home.join(dir).is_dir()
}

fn claude_status(env: &Env) -> AgentStatus {
    let settings = json(&claude::settings_path(env));
    let registered = ojson::get(
        &json(&claude::claude_json_path(env)),
        &["mcpServers", "inker"],
    )
    .is_some();
    let deny = ojson::get(&settings, &["permissions", "deny"]);
    let deny_ok = claude::deny_rules(env)
        .iter()
        .all(|r| deny.is_some_and(|d| d.contains_str(r)));
    let sandbox_ok = ojson::get(&settings, &["sandbox", "filesystem", "denyRead"])
        .is_some_and(|d| d.contains_str(&claude::sandbox_path(env)));
    let hook_ok = ojson::get(&settings, &["hooks", "PreToolUse"])
        .is_some_and(|h| h.contains_str(&env.guard_command(false)));
    AgentStatus {
        agent: Agent::ClaudeCode,
        installed: installed(env, "claude", ".claude"),
        registered,
        layers: vec![
            ("permissions.deny", deny_ok),
            ("sandbox.denyRead", sandbox_ok),
            ("PreToolUse hook", hook_ok),
            ("CLAUDE.md line", has_block(&claude::claude_md_path(env))),
        ],
    }
}

fn cursor_status(env: &Env) -> AgentStatus {
    let hooks = json(&cursor::hooks_path(env));
    let cmd = env.guard_command(true);
    let on =
        |event: &str| ojson::get(&hooks, &["hooks", event]).is_some_and(|h| h.contains_str(&cmd));
    AgentStatus {
        agent: Agent::Cursor,
        installed: installed(env, "cursor", ".cursor"),
        registered: ojson::get(&json(&cursor::mcp_path(env)), &["mcpServers", "inker"]).is_some(),
        layers: vec![
            ("beforeReadFile hook", on("beforeReadFile")),
            ("beforeShellExecution hook", on("beforeShellExecution")),
        ],
    }
}

fn codex_status(env: &Env) -> AgentStatus {
    let config: Option<toml_edit::DocumentMut> = read_optional(&codex::config_path(env))
        .ok()
        .flatten()
        .and_then(|t| t.parse().ok());
    let registered = config
        .as_ref()
        .is_some_and(|d| d.get("mcp_servers").and_then(|s| s.get("inker")).is_some());
    let private = env.private.display().to_string();
    let profile = config.as_ref().is_some_and(|d| {
        d.get("default_permissions").and_then(|v| v.as_str()) == Some(PROFILE)
            && d.get("permissions")
                .and_then(|p| p.get(PROFILE))
                .and_then(|p| p.get("filesystem"))
                .and_then(|f| f.get(&private))
                .and_then(|v| v.as_str())
                == Some("deny")
    });
    let hook = ojson::get(&json(&codex::hooks_path(env)), &["hooks", "PreToolUse"])
        .is_some_and(|h| h.contains_str(&env.guard_command(false)));
    AgentStatus {
        agent: Agent::Codex,
        installed: installed(env, "codex", ".codex"),
        registered,
        layers: vec![
            ("permission profile", profile),
            ("PreToolUse hook", hook),
            ("AGENTS.md line", has_block(&codex::agents_md_path(env))),
        ],
    }
}

/// Status of every agent, in [`Agent::ALL`] order.
pub fn doctor(env: &Env) -> Vec<AgentStatus> {
    vec![claude_status(env), cursor_status(env), codex_status(env)]
}

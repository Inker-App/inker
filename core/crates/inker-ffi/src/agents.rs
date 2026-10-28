//! Agent setup and the `doctor` report. `setup_agent` is a stub returning
//! fixed plans until the writers in `inker-agents` are wired; the doctor
//! lines come from the backend. The per-agent wording follows decision 5.3 and
//! section 3.8: none of these is called a "deny rule".

use crate::error::FfiError;

/// The agents onboarding knows how to keep out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum AgentKind {
    ClaudeCode,
    Codex,
    Cursor,
}

/// One file the writer would touch, and what it would do there.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct PlanStep {
    pub path: String,
    pub description: String,
}

/// What `setup_agent` would do (dry run) or did.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct Plan {
    pub agent: AgentKind,
    /// The onboarding row's trailing text, e.g. "read hook".
    pub summary: String,
    pub dry_run: bool,
    pub steps: Vec<PlanStep>,
}

/// The doctor report, one line per check.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct DoctorReport {
    pub lines: Vec<String>,
}

fn step(path: &str, description: &str) -> PlanStep {
    PlanStep {
        path: path.into(),
        description: description.into(),
    }
}

/// The fixed plan for each agent. `dry_run == false` is refused until the
/// real writers exist, so the stub can never touch a config file.
pub fn setup_agent(agent: AgentKind, dry_run: bool) -> Result<Plan, FfiError> {
    if !dry_run {
        return Err(FfiError::Backend {
            reason: "agent config writers are not wired yet; only dry runs are available".into(),
        });
    }
    let (summary, steps) = match agent {
        AgentKind::ClaudeCode => (
            "permission rule, sandbox and hook",
            vec![
                step(
                    "~/.claude.json",
                    "register the inker MCP server (claude mcp add --scope user)",
                ),
                step(
                    "~/.claude/settings.json",
                    "permissions.deny Read and Edit of the private folder",
                ),
                step(
                    "~/.claude/settings.json",
                    "sandbox.filesystem.denyRead for the private folder",
                ),
                step(
                    "~/.claude/settings.json",
                    "PreToolUse hook that warns on direct reads",
                ),
            ],
        ),
        AgentKind::Codex => (
            "permission profile (replaces your sandbox mode)",
            vec![
                step("~/.codex/config.toml", "register [mcp_servers.inker]"),
                step(
                    "~/.codex/config.toml",
                    "[permissions.inker.filesystem] denies the private folder; default_permissions replaces sandbox_mode",
                ),
            ],
        ),
        AgentKind::Cursor => (
            "read hook",
            vec![
                step("~/.cursor/mcp.json", "register the inker MCP server"),
                step(
                    "~/.cursor/hooks.json",
                    "beforeReadFile and beforeShellExecution hooks for the private folder",
                ),
            ],
        ),
    };
    Ok(Plan {
        agent,
        summary: summary.into(),
        dry_run,
        steps,
    })
}

/// The line for checks this report does not make yet.
pub const AGENTS_NOT_CHECKED: &str = "agents: not checked yet";

//! Agent setup over the writers in `inker-agents`, and the `doctor` report.
//! `setup_agent` plans (dry run) or applies one agent's configuration. The
//! per-agent wording follows decision 5.3 and section 3.8: none of these is
//! called a "deny rule". Chat apps come first (section 9).

use std::path::{Path, PathBuf};

use inker_agents::codex::CodexOptions;
use inker_agents::{DryRun, Env};

use crate::error::FfiError;

/// The clients INKER can register itself with. `ClaudeDesktop` is the chat
/// app and the primary target; the rest are coding agents, which also get
/// exclusion rules for the private folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum AgentKind {
    ClaudeDesktop,
    ClaudeCode,
    Codex,
    Cursor,
}

impl AgentKind {
    /// The row's trailing text in onboarding and settings.
    pub fn summary(self) -> &'static str {
        match self {
            AgentKind::ClaudeDesktop => "the Claude app on this Mac",
            AgentKind::ClaudeCode => "permission rule, sandbox and hook",
            AgentKind::Codex => "permission profile (replaces your sandbox mode)",
            AgentKind::Cursor => "read hook",
        }
    }
}

/// One file the writer would touch (or command it would run), and what it
/// would do there. Paths are shown with `~` for the home folder.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct PlanStep {
    pub path: String,
    pub description: String,
}

/// What `setup_agent` would do (dry run) or did. Empty `steps` means the
/// agent was already set up.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct Plan {
    pub agent: AgentKind,
    /// The row's trailing text, e.g. "read hook".
    pub summary: String,
    pub dry_run: bool,
    pub steps: Vec<PlanStep>,
    /// Things the user has to do, e.g. restart the Claude app.
    pub warnings: Vec<String>,
}

/// The doctor report, one line per check.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct DoctorReport {
    pub lines: Vec<String>,
}

/// One agent as `doctor` sees it, for the Settings "Chat apps" row.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct AgentReport {
    /// The app or CLI is on this Mac.
    pub installed: bool,
    /// INKER is in its MCP config (it still has to be restarted to connect).
    pub registered: bool,
}

pub fn agent_report(
    agent: AgentKind,
    inker_bin: &str,
    home: &str,
    private_folder: &str,
) -> AgentReport {
    let env = Env::detect(
        Path::new(home),
        Path::new(inker_bin),
        Path::new(private_folder),
    );
    let want = match agent {
        AgentKind::ClaudeDesktop => inker_agents::Agent::ClaudeDesktop,
        AgentKind::ClaudeCode => inker_agents::Agent::ClaudeCode,
        AgentKind::Codex => inker_agents::Agent::Codex,
        AgentKind::Cursor => inker_agents::Agent::Cursor,
    };
    inker_agents::doctor::doctor(&env)
        .into_iter()
        .find(|s| s.agent == want)
        .map(|s| AgentReport {
            installed: s.installed,
            registered: s.registered,
        })
        .unwrap_or(AgentReport {
            installed: false,
            registered: false,
        })
}

fn tilde(path: &Path, home: &Path) -> String {
    match path.strip_prefix(home) {
        Ok(rel) => format!("~/{}", rel.display()),
        Err(_) => path.display().to_string(),
    }
}

/// Plan or apply `agent`'s configuration. `inker_bin` is the CLI the client
/// should launch (the `inker-cli` inside the app bundle); `home` is the
/// user's home folder, passed in so tests can use a scratch one.
pub fn setup_agent(
    agent: AgentKind,
    dry_run: bool,
    inker_bin: &str,
    home: &str,
    private_folder: &str,
) -> Result<Plan, FfiError> {
    let home = PathBuf::from(home);
    let env = Env::detect(&home, Path::new(inker_bin), Path::new(private_folder));
    let dry = DryRun(dry_run);
    let plan = match agent {
        AgentKind::ClaudeDesktop => inker_agents::claude_desktop::setup(&env, dry),
        AgentKind::ClaudeCode => inker_agents::claude::setup(&env, dry),
        AgentKind::Codex => inker_agents::codex::setup(
            &env,
            CodexOptions {
                permission_profile: true,
            },
            dry,
        ),
        AgentKind::Cursor => inker_agents::cursor::setup(&env, dry),
    }
    .map_err(|e| FfiError::Backend {
        reason: e.to_string(),
    })?;
    let mut steps: Vec<PlanStep> = plan
        .edits
        .iter()
        .map(|e| PlanStep {
            path: tilde(&e.path, &home),
            description: e.summary.clone(),
        })
        .collect();
    steps.extend(plan.commands.iter().map(|c| PlanStep {
        path: c.display(),
        description: c.summary.clone(),
    }));
    Ok(Plan {
        agent,
        summary: agent.summary().into(),
        dry_run,
        steps,
        warnings: plan.warnings,
    })
}

/// The line for checks this report does not make yet.
pub const AGENTS_NOT_CHECKED: &str = "agents: not checked yet";

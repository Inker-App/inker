//! inker-agents: writes MCP registration for the Claude desktop app (the
//! primary target) and, for the coding-agent tier, MCP registration,
//! exclusion rules and the teaching line for Claude Code, Cursor and Codex
//! (stack-decision.md 3.8). [`mcpb`] builds the Claude Desktop Extension.
//!
//! Every writer computes a [`Plan`] first (file edits with before/after text
//! and commands to run) so onboarding can show what will change, then applies
//! it unless [`DryRun`] is set. Re-running a writer is a no-op.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod claude;
pub mod claude_desktop;
pub mod codex;
pub mod cursor;
pub mod doctor;
pub mod env;
pub mod guard;
pub mod mcpb;
mod ojson;
pub mod plan;
pub mod teach;

pub use doctor::{AgentStatus, doctor};
pub use env::Env;
pub use plan::{CommandStep, DryRun, FileEdit, Plan, apply};
pub use teach::{TEACH_END, TEACH_START, TEACHING_LINE, teach_project};

/// The agents INKER configures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Agent {
    /// The Claude chat app for macOS. The primary target.
    ClaudeDesktop,
    ClaudeCode,
    Cursor,
    Codex,
}

impl Agent {
    /// Every agent, chat apps first.
    pub const ALL: [Agent; 4] = [
        Agent::ClaudeDesktop,
        Agent::ClaudeCode,
        Agent::Cursor,
        Agent::Codex,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Agent::ClaudeDesktop => "Claude Desktop",
            Agent::ClaudeCode => "Claude Code",
            Agent::Cursor => "Cursor",
            Agent::Codex => "Codex",
        }
    }
}

/// Errors from planning or applying config edits.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("could not parse {path}: {reason}")]
    Parse { path: String, reason: String },
    #[error("{0} changed since the plan was made; run setup again")]
    Conflict(String),
    #[error("`{command}` failed: {reason}")]
    Command { command: String, reason: String },
    #[error("zip: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

//! Claude Code (stack-decision.md 3.8):
//!
//! - Registration: `claude mcp add --scope user` when the CLI is on PATH (it
//!   owns `~/.claude.json`, and a running Claude Code can revert direct edits
//!   to that file). Without the CLI, edit `~/.claude.json` only while Claude
//!   Code is not running.
//! - Exclusion, three layers in `~/.claude/settings.json`: `permissions.deny`
//!   Read/Edit rules, `sandbox.filesystem.denyRead`, and a PreToolUse hook
//!   that runs `inker guard`.
//! - Teaching: a marked block in `~/.claude/CLAUDE.md`.

use std::path::PathBuf;

use crate::ojson::{self, J, Obj};
use crate::plan::{CommandStep, DryRun, read_optional, shell_quote};
use crate::teach::plan_block;
use crate::{Env, Error, Plan, Result};

/// Tools the PreToolUse hook runs for.
pub const HOOK_MATCHER: &str = "Read|Edit|MultiEdit|Write|NotebookEdit|Glob|Grep|Bash";

pub fn settings_path(env: &Env) -> PathBuf {
    env.home.join(".claude/settings.json")
}

pub fn claude_json_path(env: &Env) -> PathBuf {
    env.home.join(".claude.json")
}

pub fn claude_md_path(env: &Env) -> PathBuf {
    env.home.join(".claude/CLAUDE.md")
}

/// The private folder in permission-rule syntax: `~/x` under home, else
/// `//abs` (a single leading `/` would mean "relative to the settings file").
pub fn rule_path(env: &Env) -> String {
    env.private_tilde()
        .unwrap_or_else(|| format!("/{}", env.private.display()))
}

/// The private folder in sandbox-path syntax: `~/x` under home, else absolute.
pub fn sandbox_path(env: &Env) -> String {
    env.private_tilde()
        .unwrap_or_else(|| env.private.display().to_string())
}

pub fn deny_rules(env: &Env) -> [String; 2] {
    let p = rule_path(env);
    [format!("Read({p}/**)"), format!("Edit({p}/**)")]
}

pub(crate) fn parse_json(path: &std::path::Path, text: Option<&str>) -> Result<J> {
    match text {
        None => Ok(J::Obj(Obj::new())),
        Some(t) if t.trim().is_empty() => Ok(J::Obj(Obj::new())),
        Some(t) => ojson::parse(t).map_err(|reason| Error::Parse {
            path: path.display().to_string(),
            reason,
        }),
    }
}

/// Add a `{matcher?, hooks: [{type: command, command}]}` entry under
/// `hooks.<event>` unless one with that command exists. Claude Code and Codex
/// share this shape.
pub(crate) fn add_command_hook(v: &mut J, event: &str, matcher: Option<&str>, command: &str) {
    let hooks = ojson::obj_path(v, &["hooks"]);
    let list = hooks
        .entry(event.to_string())
        .or_insert_with(|| J::Arr(Vec::new()));
    if list.contains_str(command) {
        return;
    }
    if !matches!(list, J::Arr(_)) {
        *list = J::Arr(Vec::new());
    }
    let mut hook = Obj::new();
    hook.insert("type".into(), J::Str("command".into()));
    hook.insert("command".into(), J::Str(command.into()));
    let mut entry = Obj::new();
    if let Some(m) = matcher {
        entry.insert("matcher".into(), J::Str(m.into()));
    }
    entry.insert("hooks".into(), J::Arr(vec![J::Obj(hook)]));
    if let J::Arr(items) = list {
        items.push(J::Obj(entry));
    }
}

/// `~/.claude/settings.json` with INKER's three exclusion layers merged in. Pure.
pub fn settings_after(env: &Env, before: Option<&str>) -> Result<String> {
    let mut v = parse_json(&settings_path(env), before)?;
    {
        let perms = ojson::obj_path(&mut v, &["permissions"]);
        let deny = perms
            .entry("deny".to_string())
            .or_insert_with(|| J::Arr(Vec::new()));
        for rule in deny_rules(env) {
            ojson::push_unique_str(deny, &rule);
        }
    }
    {
        let fs = ojson::obj_path(&mut v, &["sandbox", "filesystem"]);
        let deny_read = fs
            .entry("denyRead".to_string())
            .or_insert_with(|| J::Arr(Vec::new()));
        ojson::push_unique_str(deny_read, &sandbox_path(env));
    }
    add_command_hook(
        &mut v,
        "PreToolUse",
        Some(HOOK_MATCHER),
        &env.guard_command(false),
    );
    let after = ojson::to_string_pretty(&v);
    // Keep the user's bytes when nothing semantic changed.
    if before.is_some_and(|b| parse_json(&settings_path(env), Some(b)).ok() == Some(v)) {
        return Ok(before.unwrap_or_default().to_string());
    }
    Ok(after)
}

fn server_entry(env: &Env) -> J {
    let mut o = Obj::new();
    o.insert("type".into(), J::Str("stdio".into()));
    o.insert(
        "command".into(),
        J::Str(env.inker_bin.display().to_string()),
    );
    o.insert("args".into(), J::str_array(&["mcp"]));
    o.insert("env".into(), J::Obj(Obj::new()));
    J::Obj(o)
}

/// Whether `~/.claude.json` registers INKER, and with which command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Registration {
    Absent,
    Current,
    Stale,
}

pub fn registration(env: &Env, claude_json: Option<&str>) -> Registration {
    let Ok(v) = parse_json(&claude_json_path(env), claude_json) else {
        return Registration::Absent;
    };
    let Some(entry) = ojson::get(&v, &["mcpServers", "inker"]) else {
        return Registration::Absent;
    };
    let command = ojson::get(entry, &["command"]).and_then(J::as_str);
    let args = ojson::get(entry, &["args"]).and_then(J::as_array);
    let expected = env.inker_bin.display().to_string();
    if command == Some(expected.as_str()) && args == Some(&vec![J::Str("mcp".into())]) {
        Registration::Current
    } else {
        Registration::Stale
    }
}

/// `~/.claude.json` with `mcpServers.inker` set. Pure.
pub fn claude_json_after(env: &Env, before: Option<&str>) -> Result<String> {
    let mut v = parse_json(&claude_json_path(env), before)?;
    ojson::obj_path(&mut v, &["mcpServers"]).insert("inker".into(), server_entry(env));
    Ok(ojson::to_string_pretty(&v))
}

fn add_command(env: &Env) -> Vec<String> {
    vec![
        "mcp".into(),
        "add".into(),
        "--scope".into(),
        "user".into(),
        "inker".into(),
        "--".into(),
        env.inker_bin.display().to_string(),
        "mcp".into(),
    ]
}

/// Plan (and unless dry, apply) the Claude Code configuration.
pub fn setup(env: &Env, dry: DryRun) -> Result<Plan> {
    let mut plan = Plan::default();

    // Exclusion layers.
    let path = settings_path(env);
    let before = read_optional(&path)?;
    let after = settings_after(env, before.as_deref())?;
    let sandbox_on = ojson::parse(&after)
        .ok()
        .and_then(|v| ojson::get(&v, &["sandbox", "enabled"]).cloned())
        == Some(J::Bool(true));
    plan.edit(
        path,
        before,
        after,
        format!("deny Read/Edit of {}, add it to the sandbox denyRead list, run `inker guard` before file and shell tools", rule_path(env)),
    );
    if !sandbox_on {
        plan.warnings.push(
            "Claude Code's sandbox is off, so sandbox.filesystem.denyRead takes effect only once \
             it is turned on (/sandbox). The deny rules and the guard hook apply now."
                .into(),
        );
    }

    // Registration.
    let cj_path = claude_json_path(env);
    let cj_before = read_optional(&cj_path)?;
    let state = registration(env, cj_before.as_deref());
    if state != Registration::Current {
        if let Some(claude) = env.which("claude") {
            if state == Registration::Stale {
                plan.commands.push(CommandStep {
                    program: claude.clone(),
                    args: ["mcp", "remove", "--scope", "user", "inker"]
                        .map(String::from)
                        .to_vec(),
                    summary: "remove the old INKER registration".into(),
                });
            }
            plan.commands.push(CommandStep {
                program: claude,
                args: add_command(env),
                summary: "register INKER's MCP server for every project".into(),
            });
        } else if env.claude_running {
            let manual: Vec<String> = add_command(env).iter().map(|a| shell_quote(a)).collect();
            plan.warnings.push(format!(
                "Claude Code is running and the `claude` command is not on PATH, so INKER did not \
                 touch ~/.claude.json. Quit Claude Code and run setup again, or run: claude {}",
                manual.join(" ")
            ));
        } else {
            let after = claude_json_after(env, cj_before.as_deref())?;
            plan.edit(
                cj_path,
                cj_before,
                after,
                "register INKER's MCP server (mcpServers.inker)",
            );
        }
    }

    // Teaching.
    plan.extend(plan_block(&claude_md_path(env))?);
    plan.finish(dry)
}

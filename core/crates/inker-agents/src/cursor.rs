//! Cursor (stack-decision.md 3.8): `~/.cursor/mcp.json` for registration and
//! `~/.cursor/hooks.json` `beforeReadFile` / `beforeShellExecution` hooks for
//! exclusion. Cursor has no deny-rule file. The teaching line goes in each
//! project's AGENTS.md through `inker teach`. Cursor's files are not edited
//! while it runs.

use std::path::PathBuf;

use crate::claude::parse_json;
use crate::ojson::{self, J, Obj};
use crate::plan::{DryRun, read_optional};
use crate::{Env, Plan, Result};

pub const HOOK_EVENTS: [&str; 2] = ["beforeReadFile", "beforeShellExecution"];

pub const NO_DENY_WARNING: &str = "Cursor has no deny rule; INKER added beforeReadFile and \
beforeShellExecution hooks that block reads of the private folder instead.";

pub const TEACH_WARNING: &str = "Cursor has no global rules file INKER can write. Run `inker teach` \
in each project to add the line that explains 🐙/ handles to its AGENTS.md.";

pub const RUNNING_WARNING: &str = "Cursor is running. Quit Cursor and run setup again; INKER does \
not edit Cursor's settings while it is open.";

pub fn mcp_path(env: &Env) -> PathBuf {
    env.home.join(".cursor/mcp.json")
}

pub fn hooks_path(env: &Env) -> PathBuf {
    env.home.join(".cursor/hooks.json")
}

fn server_entry(env: &Env) -> J {
    let mut o = Obj::new();
    o.insert(
        "command".into(),
        J::Str(env.inker_bin.display().to_string()),
    );
    o.insert("args".into(), J::str_array(&["mcp"]));
    J::Obj(o)
}

/// `~/.cursor/mcp.json` with `mcpServers.inker`. Pure.
pub fn mcp_after(env: &Env, before: Option<&str>) -> Result<String> {
    let mut v = parse_json(&mcp_path(env), before)?;
    let servers = ojson::obj_path(&mut v, &["mcpServers"]);
    if servers.get("inker") == Some(&server_entry(env)) {
        return Ok(before.unwrap_or_default().to_string());
    }
    servers.insert("inker".into(), server_entry(env));
    Ok(ojson::to_string_pretty(&v))
}

/// `~/.cursor/hooks.json` with the two guard hooks. Pure.
pub fn hooks_after(env: &Env, before: Option<&str>) -> Result<String> {
    let mut v = parse_json(&hooks_path(env), before)?;
    let command = env.guard_command(true);
    let mut changed = false;
    if let J::Obj(root) = &mut v
        && !root.contains_key("version")
    {
        root.insert("version".into(), J::Num(1.into()));
        changed = true;
    }
    let hooks = ojson::obj_path(&mut v, &["hooks"]);
    for event in HOOK_EVENTS {
        let list = hooks
            .entry(event.to_string())
            .or_insert_with(|| J::Arr(Vec::new()));
        if list.contains_str(&command) {
            continue;
        }
        if !matches!(list, J::Arr(_)) {
            *list = J::Arr(Vec::new());
        }
        if let J::Arr(items) = list {
            let mut o = Obj::new();
            o.insert("command".into(), J::Str(command.clone()));
            items.push(J::Obj(o));
            changed = true;
        }
    }
    if !changed && let Some(b) = before {
        return Ok(b.to_string());
    }
    Ok(ojson::to_string_pretty(&v))
}

/// Plan (and unless dry, apply) the Cursor configuration.
pub fn setup(env: &Env, dry: DryRun) -> Result<Plan> {
    let mut plan = Plan::default();
    if env.cursor_running {
        plan.warnings.push(RUNNING_WARNING.into());
        return Ok(plan);
    }
    let p = mcp_path(env);
    let before = read_optional(&p)?;
    let after = mcp_after(env, before.as_deref())?;
    plan.edit(
        p,
        before,
        after,
        "register INKER's MCP server (mcpServers.inker)",
    );
    let p = hooks_path(env);
    let before = read_optional(&p)?;
    let after = hooks_after(env, before.as_deref())?;
    plan.edit(
        p,
        before,
        after,
        "run `inker guard` before Cursor reads a file or runs a command",
    );
    plan.warnings.push(NO_DENY_WARNING.into());
    plan.warnings.push(TEACH_WARNING.into());
    plan.finish(dry)
}

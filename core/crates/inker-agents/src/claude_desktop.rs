//! The Claude desktop app (the chat window), INKER's primary target
//! (stack-decision.md section 1 reframing).
//!
//! Claude Desktop launches local stdio MCP servers listed under `mcpServers`
//! in `~/Library/Application Support/Claude/claude_desktop_config.json`, and
//! reads that file only at startup. The writer merges one `inker` entry and
//! leaves every other key and server as it was. The other install path, a
//! Desktop Extension, is built by [`crate::mcpb`].
//!
//! A chat app has no file tools to fence off and no global rules file, so
//! there are no exclusion or teaching layers here: the server `instructions`
//! carry the teaching.

use std::path::PathBuf;

use crate::claude::parse_json;
use crate::ojson::{self, J, Obj};
use crate::plan::{DryRun, read_optional};
use crate::{Env, Error, Plan, Result};

pub const RESTART_WARNING: &str = "Claude reads its MCP settings only when it starts. Quit Claude \
(Claude > Quit Claude, or Cmd-Q) and open it again to load INKER.";

/// `~/Library/Application Support/Claude`.
pub fn config_dir(env: &Env) -> PathBuf {
    env.home.join("Library/Application Support/Claude")
}

pub fn config_path(env: &Env) -> PathBuf {
    config_dir(env).join("claude_desktop_config.json")
}

/// The app bundle Claude installs as.
pub fn app_path(env: &Env) -> PathBuf {
    env.applications.join("Claude.app")
}

/// Installed when the app bundle or its config directory exists.
pub fn installed(env: &Env) -> bool {
    app_path(env).is_dir() || config_dir(env).is_dir()
}

/// The `mcpServers.inker` entry: the absolute shim path, `mcp`, no env.
pub fn server_entry(env: &Env) -> J {
    let mut o = Obj::new();
    o.insert(
        "command".into(),
        J::Str(env.inker_bin.display().to_string()),
    );
    o.insert("args".into(), J::str_array(&["mcp"]));
    o.insert("env".into(), J::Obj(Obj::new()));
    J::Obj(o)
}

fn refuse(env: &Env, reason: impl std::fmt::Display) -> Error {
    Error::Parse {
        path: config_path(env).display().to_string(),
        reason: format!(
            "{reason}. INKER will not overwrite it; fix or remove the file and run setup again"
        ),
    }
}

/// The config with `mcpServers.inker` merged in. Pure. Returns `before`
/// unchanged when the entry is already current, and refuses (without
/// guessing) when the file is not a JSON object or `mcpServers` is not one.
pub fn config_after(env: &Env, before: Option<&str>) -> Result<String> {
    let mut v = parse_json(&config_path(env), before).map_err(|e| match e {
        Error::Parse { reason, .. } => refuse(env, format!("it is not valid JSON ({reason})")),
        other => other,
    })?;
    let J::Obj(root) = &mut v else {
        return Err(refuse(env, "its top level is not a JSON object"));
    };
    let servers = root
        .entry("mcpServers".to_string())
        .or_insert_with(|| J::Obj(Obj::new()));
    let J::Obj(servers) = servers else {
        return Err(refuse(env, "its mcpServers value is not a JSON object"));
    };
    let entry = server_entry(env);
    if servers.get("inker") == Some(&entry) {
        return Ok(before.unwrap_or_default().to_string());
    }
    servers.insert("inker".into(), entry);
    Ok(ojson::to_string_pretty(&v))
}

/// Plan (and unless dry, apply) the Claude Desktop registration.
pub fn setup(env: &Env, dry: DryRun) -> Result<Plan> {
    let mut plan = Plan::default();
    let path = config_path(env);
    let before = read_optional(&path)?;
    let after = config_after(env, before.as_deref())?;
    plan.edit(
        path,
        before,
        after,
        "register INKER's MCP server with the Claude app (mcpServers.inker)",
    );
    if !plan.is_empty() {
        plan.warnings.push(RESTART_WARNING.into());
    }
    plan.finish(dry)
}

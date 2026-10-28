//! Codex CLI (stack-decision.md 3.8): `[mcp_servers.inker]` in
//! `~/.codex/config.toml` (edited with `toml_edit` to keep the user's
//! formatting), a PreToolUse hook in `~/.codex/hooks.json`, the teaching line
//! in `~/.codex/AGENTS.md`, and, only on explicit opt-in, a permission profile
//! that denies the private folder and becomes `default_permissions`.

use std::path::PathBuf;

use toml_edit::{DocumentMut, Item, Table, value};

use crate::claude::{add_command_hook, parse_json};
use crate::ojson;
use crate::plan::{DryRun, read_optional};
use crate::teach::plan_block;
use crate::{Env, Error, Plan, Result};

/// The profile name INKER writes.
pub const PROFILE: &str = "inker";

/// Shown whenever the permission profile is written.
pub const PROFILE_WARNING: &str = "Codex: INKER set default_permissions = \"inker\". This replaces \
your sandbox_mode setting for every Codex session, and the inker profile only says that the \
private folder is denied. Remove default_permissions from ~/.codex/config.toml to go back.";

/// Codex choices the user makes in onboarding.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CodexOptions {
    /// Write the `[permissions.inker]` profile and make it the default. This
    /// replaces the user's `sandbox_mode`; only with explicit consent.
    pub permission_profile: bool,
}

pub fn config_path(env: &Env) -> PathBuf {
    env.home.join(".codex/config.toml")
}

pub fn hooks_path(env: &Env) -> PathBuf {
    env.home.join(".codex/hooks.json")
}

pub fn agents_md_path(env: &Env) -> PathBuf {
    env.home.join(".codex/AGENTS.md")
}

fn implicit_table(doc: &mut Table, key: &str) -> Result<()> {
    if !doc.contains_key(key) {
        let mut t = Table::new();
        t.set_implicit(true);
        doc.insert(key, Item::Table(t));
    }
    Ok(())
}

fn table_mut<'a>(t: &'a mut Table, key: &str) -> Option<&'a mut Table> {
    t.get_mut(key).and_then(Item::as_table_mut)
}

/// `config.toml` after INKER's edits, plus warnings. Pure.
pub fn config_after(
    env: &Env,
    before: Option<&str>,
    opts: CodexOptions,
) -> Result<(String, Vec<String>)> {
    let path = config_path(env);
    let mut doc: DocumentMut =
        before
            .unwrap_or_default()
            .parse()
            .map_err(|e: toml_edit::TomlError| Error::Parse {
                path: path.display().to_string(),
                reason: e.to_string(),
            })?;
    let mut warnings = Vec::new();
    let bin = env.inker_bin.display().to_string();
    let bad_shape = || Error::Parse {
        path: path.display().to_string(),
        reason: "unexpected table shape".into(),
    };

    // Registration.
    let current =
        doc.get("mcp_servers")
            .and_then(|s| s.get("inker"))
            .map(|i| {
                let cmd = i.get("command").and_then(Item::as_str) == Some(bin.as_str());
                let args = i.get("args").and_then(Item::as_array).is_some_and(|a| {
                    a.len() == 1 && a.get(0).and_then(|v| v.as_str()) == Some("mcp")
                });
                cmd && args
            })
            .unwrap_or(false);
    if !current {
        implicit_table(doc.as_table_mut(), "mcp_servers")?;
        let servers = table_mut(doc.as_table_mut(), "mcp_servers").ok_or_else(bad_shape)?;
        let mut t = Table::new();
        t.insert("command", value(bin.clone()));
        let mut args = toml_edit::Array::new();
        args.push("mcp");
        t.insert("args", value(args));
        servers.insert("inker", Item::Table(t));
    }

    // Opt-in permission profile.
    if opts.permission_profile {
        let private = env.private.display().to_string();
        implicit_table(doc.as_table_mut(), "permissions")?;
        let perms = table_mut(doc.as_table_mut(), "permissions").ok_or_else(bad_shape)?;
        implicit_table(perms, PROFILE)?;
        let profile = table_mut(perms, PROFILE).ok_or_else(bad_shape)?;
        if !profile.contains_key("filesystem") {
            profile.insert("filesystem", Item::Table(Table::new()));
        }
        let fs = table_mut(profile, "filesystem").ok_or_else(bad_shape)?;
        if fs.get(&private).and_then(Item::as_str) != Some("deny") {
            fs.insert(&private, value("deny"));
        }
        match doc.get("default_permissions").and_then(Item::as_str) {
            None => {
                doc.insert("default_permissions", value(PROFILE));
                warnings.push(PROFILE_WARNING.to_string());
            }
            Some(PROFILE) => warnings.push(PROFILE_WARNING.to_string()),
            Some(other) => warnings.push(format!(
                "Codex already uses default_permissions = \"{other}\". INKER wrote the \"inker\" \
                 profile but did not switch to it; add the private folder to \"{other}\" as \
                 \"deny\" instead, or switch yourself."
            )),
        }
    }

    let after = doc.to_string();
    Ok((after, warnings))
}

/// `~/.codex/hooks.json` with the PreToolUse guard. Pure.
pub fn hooks_after(env: &Env, before: Option<&str>) -> Result<String> {
    let mut v = parse_json(&hooks_path(env), before)?;
    let original = v.clone();
    add_command_hook(&mut v, "PreToolUse", None, &env.guard_command(false));
    if v == original
        && let Some(b) = before
    {
        return Ok(b.to_string());
    }
    Ok(ojson::to_string_pretty(&v))
}

/// Plan (and unless dry, apply) the Codex configuration.
pub fn setup(env: &Env, opts: CodexOptions, dry: DryRun) -> Result<Plan> {
    let mut plan = Plan::default();
    let p = config_path(env);
    let before = read_optional(&p)?;
    let (after, warnings) = config_after(env, before.as_deref(), opts)?;
    let summary = if opts.permission_profile {
        "register INKER's MCP server and deny the private folder in an \"inker\" permission profile"
    } else {
        "register INKER's MCP server ([mcp_servers.inker])"
    };
    plan.edit(p, before, after, summary);
    plan.warnings.extend(warnings);
    let p = hooks_path(env);
    let before = read_optional(&p)?;
    let after = hooks_after(env, before.as_deref())?;
    plan.edit(p, before, after, "run `inker guard` before Codex tools");
    plan.extend(plan_block(&agents_md_path(env))?);
    plan.finish(dry)
}

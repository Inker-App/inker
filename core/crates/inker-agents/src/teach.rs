//! The teaching line for CLAUDE.md / AGENTS.md.

use std::path::Path;

use crate::plan::{DryRun, read_optional};
use crate::{Plan, Result};

pub const TEACH_START: &str = "<!-- inker -->";
pub const TEACH_END: &str = "<!-- /inker -->";
pub const TEACHING_LINE: &str = "Strings that start with 🐙/ or ink/ are private document handles managed by INKER: \
call the `resolve` tool of the inker MCP server with the handle to read one, and never guess its \
contents or look for the file on disk.";

fn block() -> String {
    format!("{TEACH_START}\n{TEACHING_LINE}\n{TEACH_END}\n")
}

/// Return `existing` with the marked block added or replaced. Pure.
pub fn with_teaching_block(existing: Option<&str>) -> String {
    let existing = existing.unwrap_or_default();
    if let Some(start) = existing.find(TEACH_START)
        && let Some(end_rel) = existing[start..].find(TEACH_END)
    {
        let mut end = start + end_rel + TEACH_END.len();
        if existing[end..].starts_with('\n') {
            end += 1;
        }
        return format!("{}{}{}", &existing[..start], block(), &existing[end..]);
    }
    if existing.is_empty() {
        return block();
    }
    let sep = if existing.ends_with("\n\n") {
        ""
    } else if existing.ends_with('\n') {
        "\n"
    } else {
        "\n\n"
    };
    format!("{existing}{sep}{}", block())
}

/// Plan the edit that puts the block in `file`.
pub fn plan_block(file: &Path) -> Result<Plan> {
    let before = read_optional(file)?;
    let after = with_teaching_block(before.as_deref());
    let mut plan = Plan::default();
    plan.edit(
        file.to_path_buf(),
        before,
        after,
        "add the INKER line that explains 🐙/ handles",
    );
    Ok(plan)
}

/// `inker teach`: write the line into `<dir>/AGENTS.md`.
pub fn teach_project(dir: &Path, dry: DryRun) -> Result<Plan> {
    plan_block(&dir.join("AGENTS.md"))?.finish(dry)
}

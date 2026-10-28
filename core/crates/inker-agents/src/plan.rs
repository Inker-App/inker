//! Plans: what a writer will change, shown before it changes it.

use std::path::{Path, PathBuf};

/// Whether to only compute the plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DryRun(pub bool);

/// One file edit. `before` is `None` when the file does not exist yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileEdit {
    pub path: PathBuf,
    pub before: Option<String>,
    pub after: String,
    /// One line for the UI, e.g. "add Read and Edit deny rules for ~/Private".
    pub summary: String,
}

/// A command to run (e.g. `claude mcp add`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandStep {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub summary: String,
}

impl CommandStep {
    /// The command as a shell line, for display.
    pub fn display(&self) -> String {
        let mut parts = vec![shell_quote(&self.program.to_string_lossy())];
        parts.extend(self.args.iter().map(|a| shell_quote(a)));
        parts.join(" ")
    }
}

/// Everything one writer will do.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Plan {
    pub edits: Vec<FileEdit>,
    pub commands: Vec<CommandStep>,
    /// Sentences the UI must show (e.g. the Codex sandbox warning).
    pub warnings: Vec<String>,
}

impl Plan {
    /// True when applying would change nothing.
    pub fn is_empty(&self) -> bool {
        self.edits.is_empty() && self.commands.is_empty()
    }

    pub fn extend(&mut self, other: Plan) {
        self.edits.extend(other.edits);
        self.commands.extend(other.commands);
        self.warnings.extend(other.warnings);
    }

    /// Add an edit unless it changes nothing.
    pub fn edit(
        &mut self,
        path: PathBuf,
        before: Option<String>,
        after: String,
        summary: impl Into<String>,
    ) {
        if before.as_deref() != Some(after.as_str()) {
            self.edits.push(FileEdit {
                path,
                before,
                after,
                summary: summary.into(),
            });
        }
    }

    /// Apply unless `dry`; return self either way.
    pub fn finish(self, dry: DryRun) -> crate::Result<Plan> {
        if !dry.0 {
            apply(&self)?;
        }
        Ok(self)
    }
}

/// Write the edits (atomically, refusing if a file changed since planning)
/// and run the commands.
pub fn apply(plan: &Plan) -> crate::Result<()> {
    for e in &plan.edits {
        write_edit(e)?;
    }
    for c in &plan.commands {
        let out = std::process::Command::new(&c.program)
            .args(&c.args)
            .stdin(std::process::Stdio::null())
            .output()
            .map_err(|e| crate::Error::Command {
                command: c.display(),
                reason: e.to_string(),
            })?;
        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr);
            return Err(crate::Error::Command {
                command: c.display(),
                reason: format!("{}: {}", out.status, stderr.trim()),
            });
        }
    }
    Ok(())
}

/// Read a file that may not exist.
pub fn read_optional(path: &Path) -> crate::Result<Option<String>> {
    match std::fs::read_to_string(path) {
        Ok(s) => Ok(Some(s)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

fn write_edit(e: &FileEdit) -> crate::Result<()> {
    let current = read_optional(&e.path)?;
    if current != e.before {
        return Err(crate::Error::Conflict(e.path.display().to_string()));
    }
    let dir = e.path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(dir)?;
    let name = e
        .path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let tmp = dir.join(format!(".{name}.inker-{}", std::process::id()));
    std::fs::write(&tmp, &e.after)?;
    if let Ok(meta) = std::fs::metadata(&e.path) {
        std::fs::set_permissions(&tmp, meta.permissions())?;
    }
    std::fs::rename(&tmp, &e.path).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })?;
    tracing::info!(file = %e.path.display(), "updated");
    Ok(())
}

/// Quote a string for a POSIX shell command line.
pub fn shell_quote(s: &str) -> String {
    let safe = !s.is_empty()
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"/._-+=:,@%".contains(&b));
    if safe {
        s.to_string()
    } else {
        format!("'{}'", s.replace('\'', "'\\''"))
    }
}

//! What the writers need to know about the machine.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use crate::plan::shell_quote;

/// Machine facts, detected once. Tests build one with [`Env::for_home`].
#[derive(Debug, Clone)]
pub struct Env {
    pub home: PathBuf,
    /// Absolute path of the `inker` binary agents should launch.
    pub inker_bin: PathBuf,
    /// Absolute path of the private folder.
    pub private: PathBuf,
    /// `$PATH`, for finding agent binaries.
    pub path_var: Option<OsString>,
    pub claude_running: bool,
    pub cursor_running: bool,
}

impl Env {
    /// A hermetic environment: given home, empty PATH, nothing running.
    pub fn for_home(home: &Path, inker_bin: &Path, private: &Path) -> Self {
        Self {
            home: home.to_path_buf(),
            inker_bin: inker_bin.to_path_buf(),
            private: private.to_path_buf(),
            path_var: None,
            claude_running: false,
            cursor_running: false,
        }
    }

    /// Detect from the real process environment.
    pub fn detect(home: &Path, inker_bin: &Path, private: &Path) -> Self {
        Self {
            path_var: std::env::var_os("PATH"),
            claude_running: process_running("claude"),
            cursor_running: process_running(if cfg!(target_os = "macos") {
                "Cursor"
            } else {
                "cursor"
            }),
            ..Self::for_home(home, inker_bin, private)
        }
    }

    /// Find an executable on `path_var`.
    pub fn which(&self, name: &str) -> Option<PathBuf> {
        use std::os::unix::fs::PermissionsExt;
        let path = self.path_var.as_ref()?;
        std::env::split_paths(path).map(|d| d.join(name)).find(|p| {
            std::fs::metadata(p).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        })
    }

    /// The guard hook command line for this machine.
    pub fn guard_command(&self, cursor_format: bool) -> String {
        let bin = shell_quote(&self.inker_bin.to_string_lossy());
        let private = shell_quote(&self.private.to_string_lossy());
        if cursor_format {
            format!("{bin} guard --format cursor --path {private}")
        } else {
            format!("{bin} guard --path {private}")
        }
    }

    /// `~/rel` when the private folder is under home, else `None`.
    pub fn private_tilde(&self) -> Option<String> {
        let rel = self.private.strip_prefix(&self.home).ok()?;
        Some(format!("~/{}", rel.display()))
    }
}

/// Whether a process with exactly this name is running (`pgrep -x`).
pub fn process_running(name: &str) -> bool {
    std::process::Command::new("pgrep")
        .args(["-x", name])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

//! `inker guard`: the PreToolUse / beforeReadFile hook helper.
//!
//! Reads the hook payload (Claude Code and Codex `PreToolUse`, Cursor
//! `beforeReadFile` / `beforeShellExecution`) and denies any tool call that
//! names the private folder. It is a guardrail, not a sandbox: a command can
//! always reach the folder in ways no string match sees (variables, globs).
//! Malformed input is allowed, so a payload format change cannot brick the agent.

use std::path::{Component, Path, PathBuf};

use serde_json::Value;

/// Keys whose string values are file paths.
const PATH_KEYS: &[&str] = &[
    "file_path",
    "path",
    "notebook_path",
    "filePath",
    "file",
    "cwd_path",
];

/// What the hook decided.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    Allow,
    Deny(String),
}

/// Decide on one hook payload (Claude Code, Codex or Cursor JSON).
pub fn decide(input: &str, private: &Path, home: &Path) -> Decision {
    let Ok(v) = serde_json::from_str::<Value>(input) else {
        return Decision::Allow;
    };
    let cwd = v.get("cwd").and_then(Value::as_str).map(PathBuf::from);
    let private = normalize(private);
    let mut forms = vec![private.to_string_lossy().into_owned()];
    if let Ok(rel) = private.strip_prefix(normalize(home)) {
        let rel = rel.to_string_lossy();
        forms.extend([
            format!("~/{rel}"),
            format!("$HOME/{rel}"),
            format!("${{HOME}}/{rel}"),
        ]);
    }
    if hits(&v, None, &private, &forms, cwd.as_deref()) {
        Decision::Deny(reason(&private))
    } else {
        Decision::Allow
    }
}

fn reason(private: &Path) -> String {
    format!(
        "INKER: {} holds private documents. Do not read it directly; use the inker MCP tools \
         instead (call `resolve` with a 🐙/ or ink/ handle, or `list` to see them).",
        private.display()
    )
}

fn hits(
    v: &Value,
    key: Option<&str>,
    private: &Path,
    forms: &[String],
    cwd: Option<&Path>,
) -> bool {
    match v {
        Value::String(s) => {
            if forms.iter().any(|f| mentions(s, f)) {
                return true;
            }
            if key.is_some_and(|k| PATH_KEYS.contains(&k)) {
                let p = Path::new(s);
                let abs = match (p.is_absolute(), cwd) {
                    (true, _) => p.to_path_buf(),
                    (false, Some(c)) => c.join(p),
                    (false, None) => return false,
                };
                return normalize(&abs).starts_with(private);
            }
            false
        }
        Value::Array(a) => a.iter().any(|x| hits(x, key, private, forms, cwd)),
        Value::Object(o) => o.iter().any(|(k, x)| hits(x, Some(k), private, forms, cwd)),
        _ => false,
    }
}

/// `s` contains `form` as a whole path prefix (not `~/PrivateStuff` for `~/Private`).
fn mentions(s: &str, form: &str) -> bool {
    let is_name_char = |c: char| c.is_alphanumeric() || matches!(c, '.' | '_' | '-');
    s.match_indices(form).any(|(i, m)| {
        s[i + m.len()..]
            .chars()
            .next()
            .is_none_or(|c| !is_name_char(c))
    })
}

/// Lexical normalization: drop `.`, resolve `..`. No filesystem access.
fn normalize(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    out
}

/// The JSON Cursor expects on stdout.
pub fn cursor_response(d: &Decision) -> String {
    match d {
        Decision::Allow => r#"{"permission":"allow"}"#.to_string(),
        Decision::Deny(r) => {
            serde_json::json!({"permission": "deny", "user_message": r, "agent_message": r})
                .to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const HOME: &str = "/Users/u";
    const PRIVATE: &str = "/Users/u/Private";

    fn d(v: serde_json::Value) -> Decision {
        decide(&v.to_string(), Path::new(PRIVATE), Path::new(HOME))
    }

    fn denied(v: serde_json::Value) -> bool {
        matches!(d(v), Decision::Deny(_))
    }

    #[test]
    fn claude_read_inside_private_is_denied() {
        assert!(denied(
            json!({"hook_event_name": "PreToolUse", "tool_name": "Read", "cwd": "/Users/u/code", "tool_input": {"file_path": "/Users/u/Private/w2.pdf"}})
        ));
        assert!(denied(
            json!({"tool_name": "Read", "tool_input": {"file_path": "/Users/u/Private"}})
        ));
    }

    #[test]
    fn claude_read_elsewhere_is_allowed() {
        assert_eq!(
            d(
                json!({"tool_name": "Read", "cwd": "/Users/u/code", "tool_input": {"file_path": "/Users/u/code/main.rs"}})
            ),
            Decision::Allow
        );
        // A sibling whose name merely starts with the same characters.
        assert_eq!(
            d(json!({"tool_name": "Read", "tool_input": {"file_path": "/Users/u/PrivateStuff/a"}})),
            Decision::Allow
        );
    }

    #[test]
    fn relative_and_dotdot_paths_resolve_against_cwd() {
        assert!(denied(
            json!({"tool_name": "Read", "cwd": "/Users/u", "tool_input": {"file_path": "Private/w2.pdf"}})
        ));
        assert!(denied(
            json!({"tool_name": "Edit", "cwd": "/Users/u/code", "tool_input": {"file_path": "../Private/x"}})
        ));
        assert!(denied(
            json!({"tool_name": "Glob", "cwd": "/Users/u/code", "tool_input": {"pattern": "**/*.pdf", "path": "/Users/u/code/../Private"}})
        ));
    }

    #[test]
    fn bash_commands_mentioning_the_folder_are_denied() {
        assert!(denied(
            json!({"tool_name": "Bash", "tool_input": {"command": "cat /Users/u/Private/w2.pdf | head"}})
        ));
        assert!(denied(
            json!({"tool_name": "Bash", "tool_input": {"command": "ls ~/Private"}})
        ));
        assert!(denied(
            json!({"tool_name": "Bash", "tool_input": {"command": "grep -r x \"$HOME/Private\""}})
        ));
        assert_eq!(
            d(json!({"tool_name": "Bash", "tool_input": {"command": "ls ~/PrivateStuff"}})),
            Decision::Allow
        );
    }

    #[test]
    fn cursor_payloads() {
        assert!(denied(
            json!({"hook_event_name": "beforeReadFile", "file_path": "/Users/u/Private/a.pdf", "content": "..."})
        ));
        assert!(denied(
            json!({"hook_event_name": "beforeShellExecution", "command": "open ~/Private", "cwd": "/Users/u/proj"})
        ));
        assert_eq!(
            d(json!({"hook_event_name": "beforeReadFile", "file_path": "/Users/u/proj/a.ts"})),
            Decision::Allow
        );
    }

    #[test]
    fn codex_payload() {
        assert!(denied(
            json!({"hook_event_name": "PreToolUse", "tool_name": "shell", "tool_input": {"command": ["bash", "-lc", "cat ~/Private/x"]}})
        ));
    }

    #[test]
    fn deny_reason_points_at_the_tools() {
        let Decision::Deny(reason) = d(json!({"tool_input": {"file_path": "/Users/u/Private/a"}}))
        else {
            panic!()
        };
        assert!(
            reason.contains("resolve") && reason.contains("🐙/"),
            "{reason}"
        );
    }

    #[test]
    fn garbage_input_is_allowed() {
        assert_eq!(
            decide("not json", Path::new(PRIVATE), Path::new(HOME)),
            Decision::Allow
        );
        assert_eq!(
            decide("", Path::new(PRIVATE), Path::new(HOME)),
            Decision::Allow
        );
    }

    #[test]
    fn private_outside_home() {
        let v = json!({"tool_input": {"command": "cat /Volumes/V/Private/a"}}).to_string();
        assert!(matches!(
            decide(&v, Path::new("/Volumes/V/Private"), Path::new(HOME)),
            Decision::Deny(_)
        ));
    }

    #[test]
    fn cursor_response_json() {
        assert_eq!(
            cursor_response(&Decision::Allow),
            r#"{"permission":"allow"}"#
        );
        let v: serde_json::Value =
            serde_json::from_str(&cursor_response(&Decision::Deny("no".into()))).unwrap();
        assert_eq!(v["permission"], json!("deny"));
        assert_eq!(v["user_message"], json!("no"));
        assert_eq!(v["agent_message"], json!("no"));
    }
}

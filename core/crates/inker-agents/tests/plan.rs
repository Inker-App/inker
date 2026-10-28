#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::os::unix::fs::PermissionsExt;

use inker_agents::plan::shell_quote;
use inker_agents::{CommandStep, FileEdit, Plan, apply};

fn edit(path: std::path::PathBuf, before: Option<&str>, after: &str) -> FileEdit {
    FileEdit {
        path,
        before: before.map(str::to_string),
        after: after.to_string(),
        summary: "s".into(),
    }
}

#[test]
fn apply_creates_parents_and_writes() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("a/b/c.json");
    apply(&Plan {
        edits: vec![edit(p.clone(), None, "{}\n")],
        ..Default::default()
    })
    .unwrap();
    assert_eq!(std::fs::read_to_string(p).unwrap(), "{}\n");
}

#[test]
fn apply_refuses_when_file_changed_since_planning() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("x");
    std::fs::write(&p, "changed").unwrap();
    let err = apply(&Plan {
        edits: vec![edit(p.clone(), Some("original"), "new")],
        ..Default::default()
    })
    .unwrap_err();
    assert!(matches!(err, inker_agents::Error::Conflict(_)));
    assert_eq!(std::fs::read_to_string(p).unwrap(), "changed");
    // And a file that appeared after a "does not exist" plan.
    let q = d.path().join("y");
    std::fs::write(&q, "appeared").unwrap();
    assert!(
        apply(&Plan {
            edits: vec![edit(q, None, "new")],
            ..Default::default()
        })
        .is_err()
    );
}

#[test]
fn apply_keeps_file_mode() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("secret.json");
    std::fs::write(&p, "{}").unwrap();
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o600)).unwrap();
    apply(&Plan {
        edits: vec![edit(p.clone(), Some("{}"), "{\"a\":1}")],
        ..Default::default()
    })
    .unwrap();
    assert_eq!(
        std::fs::metadata(&p).unwrap().permissions().mode() & 0o777,
        0o600
    );
}

#[test]
fn apply_runs_commands_and_reports_failure() {
    let ok = CommandStep {
        program: "/bin/true".into(),
        args: vec![],
        summary: "ok".into(),
    };
    apply(&Plan {
        commands: vec![ok],
        ..Default::default()
    })
    .unwrap();
    let bad = CommandStep {
        program: "/bin/false".into(),
        args: vec!["x".into()],
        summary: "bad".into(),
    };
    let err = apply(&Plan {
        commands: vec![bad],
        ..Default::default()
    })
    .unwrap_err();
    assert!(matches!(err, inker_agents::Error::Command { .. }));
}

#[test]
fn plan_emptiness_and_extend() {
    let mut a = Plan::default();
    assert!(a.is_empty());
    a.warnings.push("only a warning".into());
    assert!(a.is_empty(), "warnings alone change nothing");
    let b = Plan {
        edits: vec![edit("/x".into(), None, "y")],
        ..Default::default()
    };
    a.extend(b);
    assert!(!a.is_empty());
    assert_eq!(a.warnings.len(), 1);
}

#[test]
fn quoting_and_display() {
    assert_eq!(shell_quote("/opt/inker/bin/inker"), "/opt/inker/bin/inker");
    assert_eq!(shell_quote("/a b/c"), "'/a b/c'");
    assert_eq!(shell_quote("it's"), "'it'\\''s'");
    assert_eq!(shell_quote(""), "''");
    let c = CommandStep {
        program: "/usr/bin/claude".into(),
        args: vec!["mcp".into(), "a b".into()],
        summary: String::new(),
    };
    assert_eq!(c.display(), "/usr/bin/claude mcp 'a b'");
}

#![allow(clippy::unwrap_used, clippy::expect_used)]

use inker_agents::teach::with_teaching_block;
use inker_agents::{DryRun, TEACH_END, TEACH_START, TEACHING_LINE, teach_project};

#[test]
fn block_is_marked_and_appended() {
    let out = with_teaching_block(Some("# Project\n\nRules.\n"));
    assert_eq!(
        out,
        format!("# Project\n\nRules.\n\n{TEACH_START}\n{TEACHING_LINE}\n{TEACH_END}\n")
    );
}

#[test]
fn block_on_empty_file() {
    assert_eq!(
        with_teaching_block(None),
        format!("{TEACH_START}\n{TEACHING_LINE}\n{TEACH_END}\n")
    );
}

#[test]
fn existing_block_is_replaced_in_place() {
    let old = format!("top\n{TEACH_START}\nold wording\n{TEACH_END}\nbottom\n");
    let out = with_teaching_block(Some(&old));
    assert_eq!(
        out,
        format!("top\n{TEACH_START}\n{TEACHING_LINE}\n{TEACH_END}\nbottom\n")
    );
    assert_eq!(with_teaching_block(Some(&out)), out);
}

#[test]
fn teaching_line_mentions_handles_and_resolve() {
    assert!(
        TEACHING_LINE.contains("🐙/")
            && TEACHING_LINE.contains("ink/")
            && TEACHING_LINE.contains("resolve")
    );
    assert!(!TEACHING_LINE.contains('\n'));
}

#[test]
fn teach_project_writes_agents_md_in_dir() {
    let dir = tempfile::tempdir().unwrap();
    let plan = teach_project(dir.path(), DryRun(true)).unwrap();
    assert_eq!(plan.edits.len(), 1);
    assert!(!dir.path().join("AGENTS.md").exists());
    teach_project(dir.path(), DryRun(false)).unwrap();
    let md = std::fs::read_to_string(dir.path().join("AGENTS.md")).unwrap();
    assert!(md.contains(TEACHING_LINE));
    assert!(teach_project(dir.path(), DryRun(false)).unwrap().is_empty());
}

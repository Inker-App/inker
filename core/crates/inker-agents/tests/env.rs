#![allow(clippy::unwrap_used, clippy::expect_used)]
mod common;

use common::TestHome;

#[test]
fn which_finds_executables_on_path_only() {
    let mut t = TestHome::new();
    assert_eq!(t.env.which("claude"), None);
    let bin = t.fake_bin("claude");
    assert_eq!(t.env.which("claude"), Some(bin.clone()));
    // Not executable: ignored.
    std::fs::write(bin.parent().unwrap().join("codex"), "").unwrap();
    assert_eq!(t.env.which("codex"), None);
}

#[test]
fn guard_command_quotes_and_formats() {
    let mut t = TestHome::new();
    t.env.private = "/p q/Private".into();
    assert_eq!(
        t.env.guard_command(false),
        "/opt/inker/bin/inker guard --path '/p q/Private'"
    );
    assert_eq!(
        t.env.guard_command(true),
        "/opt/inker/bin/inker guard --format cursor --path '/p q/Private'"
    );
}

#[test]
fn detect_reads_the_process_environment() {
    let home = tempfile::tempdir().unwrap();
    let env = inker_agents::Env::detect(
        home.path(),
        "/x/inker".as_ref(),
        &home.path().join("Private"),
    );
    assert_eq!(env.path_var, std::env::var_os("PATH"));
    assert_eq!(env.home, home.path());
}

#[test]
fn process_running_sees_this_test_binary_and_not_nonsense() {
    assert!(!inker_agents::env::process_running(
        "no-such-process-inker-xyz"
    ));
    // pgrep matches on comm, which is truncated to 15 chars on Linux.
    let me = std::env::current_exe().unwrap();
    let name: String = me
        .file_name()
        .unwrap()
        .to_string_lossy()
        .chars()
        .take(15)
        .collect();
    assert!(inker_agents::env::process_running(&name), "{name}");
}

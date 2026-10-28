//! Tests for every function the Swift shell calls, and for PreviewBackend.
//! Written before the implementation; see the TDD note in app/BUILD.md.

#![allow(clippy::unwrap_used, clippy::expect_used)] // tests may unwrap (CLAUDE.md)

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use inker_ffi::{
    AgentKind, CoreHandle, DocState, FfiError, HandleStyle, IngestMode, Loader, PreviewBackend,
    PreviewScenario, QueueItem, QueueListener, ReviewAnswer,
};

#[derive(Debug, Default)]
struct Counter(Arc<AtomicUsize>);

impl QueueListener for Counter {
    fn queue_changed(&self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

fn counter(handle: &CoreHandle) -> Arc<AtomicUsize> {
    let n = Arc::new(AtomicUsize::new(0));
    handle.set_listener(Box::new(Counter(n.clone())));
    n
}

/// A scratch directory unique to one test. Synthetic, empty files only.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("inker-ffi-test-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn touch(path: &std::path::Path) {
    std::fs::write(path, b"synthetic").unwrap();
}

fn fast_handle() -> Arc<CoreHandle> {
    CoreHandle::with_backend(Arc::new(PreviewBackend::live(
        "~/Private",
        Duration::from_millis(1),
    )))
}

fn wait_until(handle: &CoreHandle, pred: impl Fn(&[QueueItem]) -> bool) -> Vec<QueueItem> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let q = handle.queue();
        if pred(&q) {
            return q;
        }
        assert!(Instant::now() < deadline, "timed out; queue = {q:#?}");
        std::thread::sleep(Duration::from_millis(2));
    }
}

// ---------------------------------------------------------------------------
// open / status
// ---------------------------------------------------------------------------

// `open` now opens the real core and the socket: see tests/host_backend.rs
// and tests/providers.rs. The display path is unit-tested in `handle`.

// ---------------------------------------------------------------------------
// handle_string
// ---------------------------------------------------------------------------

#[test]
fn handle_string_renders_both_styles() {
    let h = CoreHandle::preview(PreviewScenario::Empty);
    assert_eq!(
        h.handle_string("w2-acme-2025".into(), HandleStyle::Octopus)
            .unwrap(),
        "🐙/w2-acme-2025"
    );
    assert_eq!(
        h.handle_string("w2-acme-2025".into(), HandleStyle::Ascii)
            .unwrap(),
        "ink/w2-acme-2025"
    );
}

#[test]
fn handle_string_rejects_bad_codenames() {
    let h = CoreHandle::preview(PreviewScenario::Empty);
    for bad in ["", "W2", "a--b", "-a", "a b"] {
        let err = h
            .handle_string(bad.into(), HandleStyle::Octopus)
            .unwrap_err();
        assert!(matches!(err, FfiError::InvalidArgument { .. }), "{bad:?}");
    }
}

// ---------------------------------------------------------------------------
// Preview scenarios: the four popover boards
// ---------------------------------------------------------------------------

#[test]
fn preview_empty_has_no_queue_and_no_reads() {
    let h = CoreHandle::preview(PreviewScenario::Empty);
    assert!(h.queue().is_empty());
    assert!(h.recent_reads(5).is_empty());
}

#[test]
fn preview_processing_matches_the_board() {
    let h = CoreHandle::preview(PreviewScenario::Processing);
    let q = h.queue();
    assert_eq!(q.len(), 1);
    let f = &q[0];
    assert!(f.is_folder);
    assert_eq!(f.codename, "taxes-2025");
    assert_eq!(
        (f.summary.total, f.summary.ready, f.summary.inking),
        (4, 1, 3)
    );
    let loaders: Vec<Loader> = f.children.iter().map(|c| c.loader).collect();
    assert_eq!(
        loaders,
        [Loader::Idle, Loader::Read, Loader::Ocr, Loader::Queued]
    );
    assert_eq!(f.children[0].state, DocState::Ready);
    assert_eq!(f.children[0].inked, 6);
    assert_eq!(
        f.children[2].status_note.as_deref(),
        Some("reading the scan")
    );
    assert_eq!(f.children[3].status_note.as_deref(), Some("queued"));
}

#[test]
fn preview_attention_holds_one_document_with_a_question_and_no_value() {
    let h = CoreHandle::preview(PreviewScenario::Attention);
    let f = &h.queue()[0];
    assert_eq!(
        (f.summary.total, f.summary.ready, f.summary.needs_attention),
        (4, 3, 1)
    );
    let held: Vec<_> = f
        .children
        .iter()
        .filter(|c| c.state == DocState::NeedsAttention)
        .collect();
    assert_eq!(held.len(), 1);
    let q = held[0].question.as_ref().expect("held doc has a question");
    assert_eq!(q.location, "page 3 of 11");
    assert_eq!(q.value_length, 9);
    assert!(held[0].path.ends_with("closing-statement.pdf"));
    assert!(q.prompt.ends_with("Ink it?"));
    // The value never crosses the FFI: no digit runs anywhere in the question.
    for s in [&q.prompt, &q.location, &q.context_before] {
        assert!(s.chars().filter(|c| c.is_ascii_digit()).count() <= 4, "{s}");
    }
    // Every other child is ready and has no question.
    assert!(
        f.children
            .iter()
            .filter(|c| c.state != DocState::NeedsAttention)
            .all(|c| c.question.is_none())
    );
}

#[test]
fn preview_ready_is_all_ready_with_23_inked_and_one_read() {
    let h = CoreHandle::preview(PreviewScenario::Ready);
    let f = &h.queue()[0];
    assert_eq!(
        (f.summary.total, f.summary.ready, f.summary.inked),
        (4, 4, 23)
    );
    assert!(f.children.iter().all(|c| c.loader == Loader::Idle));
    let reads = h.recent_reads(5);
    assert_eq!(reads.len(), 1);
    assert_eq!(reads[0].agent, "Claude Code");
    assert_eq!(reads[0].codename, "w2-acme-2025");
}

#[test]
fn recent_reads_respects_n() {
    let h = CoreHandle::preview(PreviewScenario::Ready);
    assert!(h.recent_reads(0).is_empty());
}

// ---------------------------------------------------------------------------
// answer
// ---------------------------------------------------------------------------

#[test]
fn answering_ink_it_releases_the_document_and_inks_one_more() {
    let h = CoreHandle::preview(PreviewScenario::Attention);
    let fired = counter(&h);
    let held = h.queue()[0]
        .children
        .iter()
        .find(|c| c.question.is_some())
        .cloned()
        .unwrap();
    let qid = held.question.unwrap().id;
    h.answer(qid, ReviewAnswer::InkIt).unwrap();
    let after = h.queue()[0]
        .children
        .iter()
        .find(|c| c.id == held.id)
        .cloned()
        .unwrap();
    assert_eq!(after.state, DocState::Ready);
    assert_eq!(after.inked, held.inked + 1);
    assert!(after.question.is_none());
    assert_eq!(h.queue()[0].summary.needs_attention, 0);
    assert!(fired.load(Ordering::SeqCst) >= 1);
}

#[test]
fn answering_leave_it_releases_without_inking() {
    let h = CoreHandle::preview(PreviewScenario::Attention);
    let held = h.queue()[0]
        .children
        .iter()
        .find(|c| c.question.is_some())
        .cloned()
        .unwrap();
    h.answer(held.question.unwrap().id, ReviewAnswer::LeaveIt)
        .unwrap();
    let after = h.queue()[0]
        .children
        .iter()
        .find(|c| c.id == held.id)
        .cloned()
        .unwrap();
    assert_eq!((after.state, after.inked), (DocState::Ready, held.inked));
}

#[test]
fn answering_an_unknown_question_is_not_found() {
    let h = CoreHandle::preview(PreviewScenario::Attention);
    let err = h.answer("nope".into(), ReviewAnswer::InkIt).unwrap_err();
    assert!(matches!(err, FfiError::NotFound { .. }));
}

// ---------------------------------------------------------------------------
// ingest (live preview)
// ---------------------------------------------------------------------------

#[test]
fn ingest_rejects_an_empty_list_and_missing_paths() {
    let h = fast_handle();
    assert!(matches!(
        h.ingest(vec![], IngestMode::Move).unwrap_err(),
        FfiError::InvalidArgument { .. }
    ));
    let err = h
        .ingest(vec!["/definitely/not/here.pdf".into()], IngestMode::Move)
        .unwrap_err();
    assert!(matches!(err, FfiError::InvalidArgument { .. }));
}

#[test]
fn ingest_a_file_queues_it_then_finishes_and_notifies() {
    let dir = scratch("file");
    let f = dir.join("W2_Acme_2025.pdf");
    touch(&f);
    let h = fast_handle();
    let fired = counter(&h);
    h.ingest(vec![f.to_string_lossy().into_owned()], IngestMode::Move)
        .unwrap();
    let q = h.queue();
    assert_eq!(q.len(), 1);
    assert!(!q[0].is_folder);
    assert_eq!(q[0].children.len(), 1);
    assert_eq!(q[0].codename, "w2-acme-2025");
    assert_eq!(q[0].children[0].kind_label, "W-2");
    assert_eq!(
        q[0].children[0].path,
        f.to_string_lossy(),
        "Open file reveals this path"
    );
    let q = wait_until(&h, |q| q[0].children.iter().all(|c| !c.state.in_progress()));
    assert_eq!(q[0].children[0].state, DocState::Ready);
    assert!(q[0].children[0].inked > 0);
    assert!(
        fired.load(Ordering::SeqCst) >= 3,
        "queued, progress and done each notify"
    );
    // The preview backend never touches the user's files.
    assert!(f.exists());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn ingest_a_folder_makes_a_folder_item_with_children_and_holds_one() {
    let dir = scratch("folder");
    let folder = dir.join("Taxes 2025");
    std::fs::create_dir_all(&folder).unwrap();
    for name in [
        "W2_Acme.pdf",
        "1099-INT_Northwind.pdf",
        "IMG_4471.heic",
        "closing-statement.pdf",
    ] {
        touch(&folder.join(name));
    }
    let h = fast_handle();
    h.ingest(
        vec![folder.to_string_lossy().into_owned()],
        IngestMode::Copy,
    )
    .unwrap();
    let q = h.queue();
    assert!(q[0].is_folder);
    assert_eq!(q[0].codename, "taxes-2025");
    assert_eq!(q[0].summary.total, 4);
    let q = wait_until(&h, |q| q[0].children.iter().all(|c| !c.state.in_progress()));
    assert_eq!(
        q[0].summary.needs_attention, 1,
        "the preview holds one file to show the card"
    );
    assert_eq!(q[0].summary.ready, 3);
    let scan = q[0]
        .children
        .iter()
        .find(|c| c.original_name == "IMG_4471.heic")
        .unwrap();
    assert_eq!(scan.kind_label, "scan");
    assert_eq!(scan.path, folder.join("IMG_4471.heic").to_string_lossy());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn newest_drop_comes_first() {
    let dir = scratch("order");
    let a = dir.join("first.pdf");
    let b = dir.join("second.pdf");
    touch(&a);
    touch(&b);
    let h = fast_handle();
    h.ingest(vec![a.to_string_lossy().into_owned()], IngestMode::Move)
        .unwrap();
    h.ingest(vec![b.to_string_lossy().into_owned()], IngestMode::Move)
        .unwrap();
    let q = h.queue();
    assert_eq!(q[0].codename, "second");
    assert_eq!(q[1].codename, "first");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn colliding_codenames_get_a_numeric_suffix() {
    // Decision 5.7: w2-acme-2025, w2-acme-2025-2.
    let dir = scratch("collide");
    let a = dir.join("w2-acme-2025.pdf");
    touch(&a);
    let h = fast_handle();
    h.ingest(vec![a.to_string_lossy().into_owned()], IngestMode::Copy)
        .unwrap();
    h.ingest(vec![a.to_string_lossy().into_owned()], IngestMode::Copy)
        .unwrap();
    let names: Vec<String> = h.queue().iter().map(|i| i.codename.clone()).collect();
    assert_eq!(names, ["w2-acme-2025-2", "w2-acme-2025"]);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn clearing_the_listener_stops_notifications() {
    let h = CoreHandle::preview(PreviewScenario::Attention);
    let fired = counter(&h);
    h.clear_listener();
    let qid = h.queue()[0]
        .children
        .iter()
        .find_map(|c| c.question.clone())
        .unwrap()
        .id;
    h.answer(qid, ReviewAnswer::InkIt).unwrap();
    assert_eq!(fired.load(Ordering::SeqCst), 0);
}

// ---------------------------------------------------------------------------
// codenames
// ---------------------------------------------------------------------------

#[test]
fn codename_from_names() {
    assert_eq!(
        inker_ffi::codename_from_name("1099-INT_Northwind.pdf"),
        "1099-int-northwind"
    );
    assert_eq!(inker_ffi::codename_from_name("Taxes 2025"), "taxes-2025");
    assert_eq!(
        inker_ffi::codename_from_name("  --IMG__4471.HEIC"),
        "img-4471"
    );
    assert_eq!(inker_ffi::codename_from_name("💸.pdf"), "document");
}

// ---------------------------------------------------------------------------
// setup_agent / doctor stubs
// ---------------------------------------------------------------------------

#[test]
fn setup_agent_plans_match_decision_5_3() {
    let h = CoreHandle::preview(PreviewScenario::Empty);
    let claude = h.setup_agent(AgentKind::ClaudeCode, true).unwrap();
    assert!(claude.dry_run);
    assert_eq!(claude.summary, "permission rule, sandbox and hook");
    assert!(
        claude
            .steps
            .iter()
            .any(|s| s.path == "~/.claude/settings.json")
    );
    let codex = h.setup_agent(AgentKind::Codex, true).unwrap();
    assert_eq!(
        codex.summary,
        "permission profile (replaces your sandbox mode)"
    );
    let cursor = h.setup_agent(AgentKind::Cursor, true).unwrap();
    assert_eq!(cursor.summary, "read hook");
    for p in [&claude, &codex, &cursor] {
        assert!(!p.summary.contains("deny rule"));
    }
}

#[test]
fn setup_agent_stub_refuses_to_write_for_real() {
    let h = CoreHandle::preview(PreviewScenario::Empty);
    assert!(matches!(
        h.setup_agent(AgentKind::Cursor, false).unwrap_err(),
        FfiError::Backend { .. }
    ));
}

#[test]
fn doctor_reports_lines() {
    let h = CoreHandle::preview(PreviewScenario::Empty);
    let r = h.doctor();
    assert!(!r.lines.is_empty());
    assert!(r.lines.iter().any(|l| l.contains("preview backend")));
}

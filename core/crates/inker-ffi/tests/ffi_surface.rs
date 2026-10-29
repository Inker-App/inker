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
    assert_eq!(reads[0].agent, "Claude");
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
// setup_agent: the real writers from inker-agents, on a scratch home
// ---------------------------------------------------------------------------

/// A handle over the real core on a scratch dir, plus a scratch home the
/// writers may edit. Nothing here touches the user's own config files.
fn setup_world(name: &str) -> (Arc<CoreHandle>, PathBuf, String) {
    let dir = scratch(name);
    let home = dir.join("home");
    std::fs::create_dir_all(&home).unwrap();
    let private = home.join("Private");
    let data = dir.join("data");
    let h = CoreHandle::open_with_rust_providers(
        private.to_string_lossy().into_owned(),
        data.to_string_lossy().into_owned(),
    )
    .unwrap();
    let bin = dir.join("INKER.app/Contents/MacOS/inker-cli");
    (h, home, bin.to_string_lossy().into_owned())
}

fn s(p: &std::path::Path) -> String {
    p.to_string_lossy().into_owned()
}

#[test]
fn setup_agent_claude_desktop_dry_run_plans_the_config_edit_and_writes_nothing() {
    let (h, home, bin) = setup_world("cd-dry");
    let plan = h
        .setup_agent(AgentKind::ClaudeDesktop, true, bin.clone(), s(&home))
        .unwrap();
    assert!(plan.dry_run);
    assert_eq!(plan.agent, AgentKind::ClaudeDesktop);
    assert_eq!(plan.summary, "the Claude app on this Mac");
    let step = &plan.steps[0];
    assert_eq!(
        step.path,
        "~/Library/Application Support/Claude/claude_desktop_config.json"
    );
    assert!(step.description.contains("mcpServers.inker"), "{step:?}");
    assert!(
        plan.warnings.iter().any(|w| w.contains("Quit Claude")),
        "{:?}",
        plan.warnings
    );
    assert!(!home.join("Library").exists(), "dry run wrote a file");
    h.close();
}

#[test]
fn setup_agent_claude_desktop_writes_the_config_pointing_at_the_bundled_cli() {
    let (h, home, bin) = setup_world("cd-write");
    let plan = h
        .setup_agent(AgentKind::ClaudeDesktop, false, bin.clone(), s(&home))
        .unwrap();
    assert!(!plan.dry_run);
    let written = std::fs::read_to_string(
        home.join("Library/Application Support/Claude/claude_desktop_config.json"),
    )
    .unwrap();
    assert!(written.contains(&bin), "{written}");
    assert!(written.contains("\"mcp\""), "{written}");
    // Running it again changes nothing.
    let again = h
        .setup_agent(AgentKind::ClaudeDesktop, false, bin, s(&home))
        .unwrap();
    assert!(again.steps.is_empty(), "{again:?}");
    assert!(again.warnings.is_empty(), "{again:?}");
    h.close();
}

#[test]
fn setup_agent_coding_agents_keep_their_row_summaries() {
    let (h, home, bin) = setup_world("coding");
    let cursor = h
        .setup_agent(AgentKind::Cursor, true, bin.clone(), s(&home))
        .unwrap();
    assert_eq!(cursor.summary, "read hook");
    assert!(
        cursor
            .steps
            .iter()
            .any(|st| st.path == "~/.cursor/hooks.json"),
        "{cursor:?}"
    );
    let codex = h
        .setup_agent(AgentKind::Codex, true, bin.clone(), s(&home))
        .unwrap();
    assert_eq!(
        codex.summary,
        "permission profile (replaces your sandbox mode)"
    );
    assert!(
        codex
            .steps
            .iter()
            .any(|st| st.path == "~/.codex/config.toml"),
        "{codex:?}"
    );
    let claude = h
        .setup_agent(AgentKind::ClaudeCode, true, bin, s(&home))
        .unwrap();
    assert_eq!(claude.summary, "permission rule, sandbox and hook");
    assert!(
        claude
            .steps
            .iter()
            .any(|st| st.path == "~/.claude/settings.json"),
        "{claude:?}"
    );
    for p in [&cursor, &codex, &claude] {
        assert!(!p.summary.contains("deny rule"));
    }
    h.close();
}

#[test]
fn setup_agent_is_refused_in_a_preview() {
    let h = CoreHandle::preview(PreviewScenario::Empty);
    assert!(matches!(
        h.setup_agent(AgentKind::Cursor, true, "/x/inker-cli".into(), "/x".into())
            .unwrap_err(),
        FfiError::Backend { .. }
    ));
}

#[test]
fn doctor_reports_lines() {
    let h = CoreHandle::preview(PreviewScenario::Empty);
    let r = h.doctor();
    assert!(!r.lines.is_empty());
    // The preview backend says what it is: no vault, no socket.
    assert!(r.lines.iter().any(|l| l.contains("preview")), "{r:?}");
}

// ---------------------------------------------------------------------------
// Finding documents: the board at scale, Find, locate, the row menu
// ---------------------------------------------------------------------------

use inker_ffi::{BoardEntry, MatchField};

fn entry_names(g: &inker_ffi::YearGroup) -> Vec<String> {
    g.entries
        .iter()
        .map(|e| match e {
            BoardEntry::Doc { item } => item.original_name.clone(),
            BoardEntry::Folder { item } => format!("folder:{}", item.name),
        })
        .collect()
}

#[test]
fn preview_scale_is_thirty_files_in_three_years_with_two_folders_and_three_chips() {
    let h = CoreHandle::preview(PreviewScenario::Scale);
    let b = h.board(None);
    assert_eq!(b.total, 30);
    let years: Vec<Option<u16>> = b.groups.iter().map(|g| g.year).collect();
    assert_eq!(years, vec![Some(2025), Some(2024), Some(2023)]);
    let files: Vec<u32> = b.groups.iter().map(|g| g.files).collect();
    assert_eq!(files, vec![14, 11, 5]);
    let tags: Vec<(String, u32)> = b.tags.iter().map(|t| (t.tag.clone(), t.count)).collect();
    assert_eq!(
        tags,
        vec![
            ("Taxes".to_string(), 24),
            ("Banking".to_string(), 5),
            ("Legal".to_string(), 1)
        ]
    );
    let names = entry_names(&b.groups[0]);
    assert!(
        names.contains(&"folder:Taxes 2025".to_string()),
        "{names:?}"
    );
    assert!(
        names.contains(&"W2_AcmeRobotics_2025.pdf".to_string()),
        "{names:?}"
    );
    let folder = b.groups[0]
        .entries
        .iter()
        .find_map(|e| match e {
            BoardEntry::Folder { item } => Some(item),
            BoardEntry::Doc { .. } => None,
        })
        .unwrap();
    assert_eq!(folder.codename, "folder-0412");
    assert_eq!(folder.children.len(), 9);
    assert_eq!(folder.summary.inked, 41);
    assert!(
        folder
            .children
            .iter()
            .all(|c| c.folder_name.as_deref() == Some("Taxes 2025"))
    );
    let w2 = b.groups[0]
        .entries
        .iter()
        .find_map(|e| match e {
            BoardEntry::Doc { item } if item.codename == "w2-2025-2" => Some(item),
            _ => None,
        })
        .unwrap();
    assert_eq!(w2.original_name, "w2-bluefin.pdf");
    assert_eq!(w2.payer.as_deref(), Some("Bluefin Analytics"));
    assert_eq!(w2.tag.as_deref(), Some("Taxes"));
    assert_eq!(w2.year, Some(2025));
    assert_eq!(w2.kind_label, "W-2");
    assert_eq!(w2.inked, 6);
    let reads = h.recent_reads(1);
    assert_eq!(reads[0].codename, "w2-2025-2");
    assert_eq!(reads[0].agent, "Claude");

    // The queue still works for the other boards: 30 ready files, all settled.
    let q = h.queue();
    assert_eq!(q.iter().map(|i| i.children.len()).sum::<usize>(), 30);
}

#[test]
fn a_chip_filters_the_board_but_the_counts_cover_everything() {
    let h = CoreHandle::preview(PreviewScenario::Scale);
    let b = h.board(Some("Banking".into()));
    assert_eq!(b.total, 30);
    assert_eq!(b.tags.len(), 3);
    let files: Vec<u32> = b.groups.iter().map(|g| g.files).collect();
    assert_eq!(files, vec![1, 2, 2]);
    assert!(b.groups.iter().all(|g| {
        g.entries.iter().all(
            |e| matches!(e, BoardEntry::Doc { item } if item.tag.as_deref() == Some("Banking")),
        )
    }));
    let legal = h.board(Some("Legal".into()));
    assert_eq!(legal.groups.len(), 1);
    assert_eq!(legal.groups[0].year, Some(2023));
}

#[test]
fn find_in_the_preview_matches_fields_and_text() {
    let h = CoreHandle::preview(PreviewScenario::Scale);
    let hits = h.find("acme".into());
    assert_eq!(hits.len(), 6, "{hits:#?}");
    assert_eq!(hits[0].doc.original_name, "W2_AcmeRobotics_2025.pdf");
    assert_eq!(hits[0].field, MatchField::FileName);
    assert_eq!(hits[0].matched, "Acme");
    assert_eq!(hits[1].doc.original_name, "w2-acme-2024.pdf");
    assert_eq!(hits[1].field, MatchField::FileName);
    assert!(
        hits[2..]
            .iter()
            .all(|m| m.field == MatchField::Payer && m.matched == "Acme"),
        "{hits:#?}"
    );

    let hits = h.find("mortgage interest".into());
    assert_eq!(hits.len(), 3, "three 1098s: {hits:#?}");
    assert!(hits.iter().all(|m| m.field == MatchField::Text));
    assert!(
        hits[0]
            .snippet
            .as_deref()
            .unwrap()
            .contains("Mortgage Interest")
    );
    assert_eq!(hits[0].matched, "Mortgage Interest");
    assert_eq!(hits[0].doc.year, Some(2025), "newest first");
    assert_eq!(hits[2].doc.year, Some(2024));

    let hits = h.find("🐙/w2-2025-2".into());
    assert_eq!(hits.len(), 1);
    assert!(hits[0].exact_handle);
    assert_eq!(h.find("ink/w2-2025-2".into())[0].doc.codename, "w2-2025-2");
    assert!(h.find("".into()).is_empty());
    assert!(h.find("Taxes".into()).len() >= 15, "tag and folder matches");
}

#[test]
fn locate_accepts_both_prefixes_and_only_handles_the_app_holds() {
    let h = CoreHandle::preview(PreviewScenario::Scale);
    let found = h.locate("🐙/w2-2025-2".into()).unwrap();
    assert!(matches!(found, BoardEntry::Doc { item } if item.codename == "w2-2025-2"));
    assert!(matches!(
        h.locate(" ink/w2-2025-2 \n".into()),
        Some(BoardEntry::Doc { .. })
    ));
    assert!(matches!(
        h.locate("🐙/folder-0412".into()),
        Some(BoardEntry::Folder { .. })
    ));
    assert!(
        h.locate("w2-2025-2".into()).is_none(),
        "a bare codename is not a handle"
    );
    assert!(h.locate("🐙/w2-2031".into()).is_none(), "not held");
    assert!(h.locate("hello".into()).is_none());
    assert!(h.locate("🐙/Not A Codename".into()).is_none());
    assert!(h.locate("".into()).is_none());
}

#[test]
fn details_describe_the_expanded_row() {
    let h = CoreHandle::preview(PreviewScenario::Scale);
    let w2 = h.locate("🐙/w2-2025-2".into()).unwrap();
    let BoardEntry::Doc { item } = w2 else {
        panic!()
    };
    let d = h.details(item.id.clone()).unwrap();
    assert_eq!(d.location, "", "a loose file sits at the root");
    assert_eq!(d.dropped_with, 0);
    assert_eq!(
        d.last_read.as_ref().map(|r| r.agent.as_str()),
        Some("Claude")
    );
    assert!(d.dropped_at <= std::time::SystemTime::now());

    let BoardEntry::Doc { item } = h.locate("🐙/1099-nec-2025".into()).unwrap() else {
        panic!()
    };
    let d = h.details(item.id).unwrap();
    assert_eq!(d.location, "Taxes 2025/");
    assert_eq!(d.dropped_with, 8, "came in with eight others");
    assert!(d.last_read.is_none());
    assert!(matches!(
        h.details("nope".into()),
        Err(FfiError::NotFound { .. })
    ));
}

#[test]
fn forget_removes_a_file_or_a_folder_and_notifies() {
    let h = CoreHandle::preview(PreviewScenario::Scale);
    let n = counter(&h);
    let BoardEntry::Doc { item } = h.locate("🐙/w2-2025-2".into()).unwrap() else {
        panic!()
    };
    h.forget(item.id.clone()).unwrap();
    assert!(h.locate("🐙/w2-2025-2".into()).is_none());
    assert_eq!(h.board(None).total, 29);
    assert!(n.load(Ordering::SeqCst) >= 1);
    assert!(matches!(h.forget(item.id), Err(FfiError::NotFound { .. })));

    let BoardEntry::Folder { item } = h.locate("🐙/folder-0412".into()).unwrap() else {
        panic!()
    };
    h.forget(item.id).unwrap();
    assert!(h.locate("🐙/folder-0412".into()).is_none());
    assert_eq!(h.board(None).total, 20);
}

#[test]
fn ink_again_requeues_and_finishes_with_the_same_handle() {
    let h = CoreHandle::with_backend(Arc::new(PreviewBackend::scenario_with_tick(
        PreviewScenario::Scale,
        Duration::from_millis(1),
    )));
    let BoardEntry::Doc { item } = h.locate("🐙/w2-2025-2".into()).unwrap() else {
        panic!()
    };
    h.ink_again(item.id.clone()).unwrap();
    let q = wait_until(&h, |q| {
        q.iter()
            .flat_map(|i| i.children.iter())
            .any(|d| d.id == item.id && d.state != DocState::Ready)
    });
    assert!(
        q.iter()
            .flat_map(|i| i.children.iter())
            .any(|d| d.id == item.id)
    );
    wait_until(&h, |q| {
        q.iter()
            .flat_map(|i| i.children.iter())
            .all(|d| d.state == DocState::Ready)
    });
    let BoardEntry::Doc { item: again } = h.locate("🐙/w2-2025-2".into()).unwrap() else {
        panic!()
    };
    assert_eq!(again.id, item.id);
    assert!(matches!(
        h.ink_again("nope".into()),
        Err(FfiError::NotFound { .. })
    ));
}

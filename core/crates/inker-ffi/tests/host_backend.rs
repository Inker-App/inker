#![allow(clippy::unwrap_used, clippy::expect_used)]
//! The FFI handle over the real core: a `Host` on a temp dir with the
//! pure-Rust extractor and the file key store, plus the MCP socket the app
//! process serves. Everything here crosses the same surface Swift uses.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use camino::Utf8PathBuf;
use inker_ffi::{
    CoreHandle, DocItem, DocState, FfiError, IngestMode, QueueItem, QueueListener, ReviewAnswer,
};
use inker_mcp::McpClient;
use serde_json::json;

const SAMPLES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../samples/synthetic");
/// The synthetic W-2's employee SSN (area 900: never issued).
const JORDAN_SSN: &str = "900-12-3456";
/// A held value: eleven zeros look like an account number but fail every
/// checksum, so the core asks.
const HELD_NOTE: &str = "Wire confirmation 00000000000 received\n";

struct Counter(Arc<AtomicUsize>);

impl QueueListener for Counter {
    fn queue_changed(&self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

struct World {
    tmp: tempfile::TempDir,
}

impl World {
    fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir_all(tmp.path().join("Downloads")).unwrap();
        Self { tmp }
    }

    fn private(&self) -> PathBuf {
        self.tmp.path().join("Private")
    }

    fn data(&self) -> PathBuf {
        self.tmp.path().join("data")
    }

    fn socket(&self) -> PathBuf {
        self.data().join("inker.sock")
    }

    fn open(&self) -> Arc<CoreHandle> {
        CoreHandle::open_with_rust_providers(s(&self.private()), s(&self.data())).unwrap()
    }

    /// Copy a synthetic sample into the fake Downloads folder.
    fn download(&self, name: &str) -> String {
        let dst = self.tmp.path().join("Downloads").join(name);
        fs::copy(Path::new(SAMPLES).join(name), &dst).unwrap();
        s(&dst)
    }

    fn download_text(&self, name: &str, text: &str) -> String {
        let dst = self.tmp.path().join("Downloads").join(name);
        fs::write(&dst, text).unwrap();
        s(&dst)
    }
}

fn s(p: &Path) -> String {
    p.to_str().unwrap().to_string()
}

fn counter(h: &CoreHandle) -> Arc<AtomicUsize> {
    let n = Arc::new(AtomicUsize::new(0));
    h.set_listener(Box::new(Counter(n.clone())));
    n
}

fn wait_for<T>(what: &str, mut f: impl FnMut() -> Option<T>) -> T {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if let Some(v) = f() {
            return v;
        }
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn wait_queue(h: &CoreHandle, pred: impl Fn(&[QueueItem]) -> bool) -> Vec<QueueItem> {
    wait_for("the queue", || {
        let q = h.queue();
        pred(&q).then_some(q)
    })
}

fn docs(q: &[QueueItem]) -> Vec<&DocItem> {
    q.iter().flat_map(|i| i.children.iter()).collect()
}

fn all_settled(q: &[QueueItem]) -> bool {
    !q.is_empty() && docs(q).iter().all(|d| !d.state.in_progress())
}

/// Every string anywhere in the queue, via `Debug`, must be free of the value.
fn assert_no_value(q: &[QueueItem]) {
    let dump = format!("{q:?}");
    assert!(!dump.contains(JORDAN_SSN), "a real value reached the queue");
    assert!(
        !dump.contains("00000000000"),
        "a held value reached the queue"
    );
}

// ---------------------------------------------------------------------------
// ingest, queue, sink
// ---------------------------------------------------------------------------

#[test]
fn ingest_returns_once_queued_and_the_sink_reports_ready() {
    let w = World::new();
    let h = w.open();
    let n = counter(&h);
    let src = w.download("w2-acme-2025.txt");

    h.ingest(vec![src.clone()], IngestMode::Move).unwrap();
    // Queued synchronously: the row is there the moment ingest returns.
    let q = h.queue();
    assert_eq!(q.len(), 1, "{q:#?}");
    assert!(
        !Path::new(&src).exists(),
        "a move takes the original out of Downloads"
    );

    let q = wait_queue(&h, |q| docs(q).iter().any(|d| d.state == DocState::Ready));
    assert!(n.load(Ordering::SeqCst) >= 1, "the sink fired");
    assert_eq!(q.len(), 1);
    let item = &q[0];
    assert!(!item.is_folder);
    assert_eq!(item.children.len(), 1);
    let d = &item.children[0];
    assert_eq!(item.codename, d.codename);
    assert!(d.codename.starts_with("w2-"), "{}", d.codename);
    assert_eq!(d.kind_label, "W-2");
    assert_eq!(d.original_name, "w2-acme-2025.txt");
    assert!(d.inked > 0);
    assert!(d.question.is_none());
    assert!(d.path.starts_with(&s(&w.private())), "{}", d.path);
    assert_eq!(item.summary.ready, 1);
    assert_no_value(&q);
}

#[test]
fn a_held_note_shows_its_question_without_the_value_and_answering_makes_it_ready() {
    let w = World::new();
    let h = w.open();
    let src = w.download_text("note.txt", HELD_NOTE);
    h.ingest(vec![src], IngestMode::Move).unwrap();

    let q = wait_queue(&h, all_settled);
    assert_no_value(&q);
    let d = docs(&q)[0].clone();
    assert_eq!(d.state, DocState::NeedsAttention, "{d:#?}");
    let question = d.question.expect("a held document carries its question");
    assert!(!question.prompt.is_empty());
    assert_eq!(question.location, "page 1 of 1");
    // The peek: the words before the value and the outline's size, never the
    // value.
    assert_eq!(question.context_before, "Wire confirmation");
    assert_eq!(question.value_length, 11);
    // The page crop: a 0-based page and the byte span in that page's text.
    assert_eq!(question.page_index, 0);
    assert_eq!(question.span.clone().map(|s| s.end - s.start), Some(11));
    let n = counter(&h);

    h.answer(question.id.clone(), ReviewAnswer::InkIt).unwrap();
    // The answer re-inks synchronously; the queue reflects it on return.
    let q = h.queue();
    let d = docs(&q)[0];
    assert_eq!(d.state, DocState::Ready, "{d:#?}");
    assert!(d.question.is_none());
    assert!(n.load(Ordering::SeqCst) >= 1, "answer fires the sink");
    assert_no_value(&q);

    let err = h.answer(question.id, ReviewAnswer::InkIt);
    assert!(err.is_err(), "a question can only be answered once");
    let err = h
        .answer("not-a-number".into(), ReviewAnswer::LeaveIt)
        .unwrap_err();
    assert!(matches!(err, FfiError::InvalidArgument { .. }), "{err:?}");
}

#[test]
fn a_folder_drop_is_one_item_with_its_children() {
    let w = World::new();
    let h = w.open();
    let dir = w.tmp.path().join("Downloads").join("taxes");
    fs::create_dir_all(&dir).unwrap();
    for name in ["w2-acme-2025.txt", "k1-tidewater-2025.txt"] {
        fs::copy(Path::new(SAMPLES).join(name), dir.join(name)).unwrap();
    }
    h.ingest(vec![s(&dir)], IngestMode::Copy).unwrap();

    let q = wait_queue(&h, |q| {
        all_settled(q) && q.first().is_some_and(|i| i.children.len() == 2)
    });
    assert!(
        dir.join("w2-acme-2025.txt").exists(),
        "a copy leaves the original"
    );
    assert_eq!(q.len(), 1, "{q:#?}");
    let item = &q[0];
    assert!(item.is_folder);
    assert_eq!(item.summary.total, 2);
    assert_eq!(item.summary.ready, 2, "{q:#?}");
    assert_eq!(
        item.summary.inked,
        item.children.iter().map(|d| d.inked).sum::<u32>()
    );
    let labels: Vec<&str> = item
        .children
        .iter()
        .map(|d| d.kind_label.as_str())
        .collect();
    assert!(labels.contains(&"W-2"), "{labels:?}");
    assert!(labels.contains(&"K-1"), "{labels:?}");
    assert_no_value(&q);
}

#[test]
fn newest_drop_comes_first() {
    let w = World::new();
    let h = w.open();
    h.ingest(vec![w.download("w2-acme-2025.txt")], IngestMode::Move)
        .unwrap();
    wait_queue(&h, all_settled);
    std::thread::sleep(Duration::from_millis(5));
    h.ingest(vec![w.download("k1-tidewater-2025.txt")], IngestMode::Move)
        .unwrap();
    let q = wait_queue(&h, |q| q.len() == 2 && all_settled(q));
    assert!(q[0].codename.starts_with("k1"), "{q:#?}");
    assert!(q[1].codename.starts_with("w2-"), "{q:#?}");
}

#[test]
fn ingest_rejects_an_empty_list_and_missing_paths_without_naming_them() {
    let w = World::new();
    let h = w.open();
    let err = h.ingest(vec![], IngestMode::Move).unwrap_err();
    assert!(matches!(err, FfiError::InvalidArgument { .. }));
    let missing = s(&w.tmp.path().join("Downloads").join("secret-name.pdf"));
    let err = h.ingest(vec![missing], IngestMode::Move).unwrap_err();
    assert!(matches!(err, FfiError::InvalidArgument { .. }));
    assert!(!err.to_string().contains("secret-name"), "{err}");
    assert!(h.queue().is_empty());
}

#[test]
fn queued_documents_left_by_a_previous_run_are_processed_on_open() {
    let w = World::new();
    let src = w.download("w2-acme-2025.txt");
    {
        let host = inker_host::Host::open(inker_host::HostConfig {
            private_folder: Utf8PathBuf::from(s(&w.private())),
            data_dir: Utf8PathBuf::from(s(&w.data())),
            packs_dir: None,
        })
        .unwrap();
        // Queued but never processed, as if the app quit mid-drop.
        host.core()
            .begin_ingest(&[Utf8PathBuf::from(src)], inker_core::IngestMode::Move)
            .unwrap();
    }
    let h = w.open();
    let q = wait_queue(&h, |q| docs(q).iter().any(|d| d.state == DocState::Ready));
    assert_eq!(docs(&q).len(), 1);
}

#[test]
fn a_document_left_inking_by_a_crash_is_processed_on_open() {
    let w = World::new();
    let src = w.download("w2-acme-2025.txt");
    {
        let host = inker_host::Host::open(inker_host::HostConfig {
            private_folder: Utf8PathBuf::from(s(&w.private())),
            data_dir: Utf8PathBuf::from(s(&w.data())),
            packs_dir: None,
        })
        .unwrap();
        // The app died while inking: the record never left `Inking`.
        let core = host.core();
        let id = core
            .begin_ingest(&[Utf8PathBuf::from(src)], inker_core::IngestMode::Move)
            .unwrap()[0];
        core.set_state(id, inker_core::DocState::Reading, Some("reading"))
            .unwrap();
        core.set_state(id, inker_core::DocState::Inking, Some("inking"))
            .unwrap();
    }
    let h = w.open();
    let q = wait_queue(&h, |q| docs(q).iter().any(|d| d.state == DocState::Ready));
    assert_eq!(docs(&q).len(), 1);
    assert_no_value(&q);
}

// ---------------------------------------------------------------------------
// status, socket, close
// ---------------------------------------------------------------------------

#[test]
fn status_reports_the_socket_listening_and_close_takes_it_down() {
    let w = World::new();
    let h = w.open();
    wait_for("the socket", || h.status().mcp_listening.then_some(()));
    assert!(w.socket().exists());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(w.socket()).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "socket mode {mode:o}");
    }
    assert_eq!(h.status().private_folder_display, s(&w.private()));

    h.close();
    assert!(!h.status().mcp_listening);
    assert!(!w.socket().exists(), "close removes the socket");
    h.close(); // idempotent
}

#[test]
fn doctor_reports_the_socket_the_data_dir_and_the_private_folder() {
    let w = World::new();
    let h = w.open();
    wait_for("the socket", || h.status().mcp_listening.then_some(()));
    let r = h.doctor();
    let has = |needle: &str| r.lines.iter().any(|l| l.contains(needle));
    assert!(has("mcp: listening"), "{r:?}");
    assert!(has(&s(&w.socket())), "{r:?}");
    assert!(has(&format!("data dir: {}", s(&w.data()))), "{r:?}");
    assert!(
        has(&format!("private folder: {}", s(&w.private()))),
        "{r:?}"
    );
    assert!(!has("preview"), "{r:?}");

    h.close();
    let r = h.doctor();
    assert!(r.lines.iter().any(|l| l == "mcp: not listening"), "{r:?}");
}

#[test]
fn dropping_the_handle_releases_the_core_for_a_reopen() {
    let w = World::new();
    let h = w.open();
    wait_for("the socket", || h.status().mcp_listening.then_some(()));
    drop(h);
    assert!(!w.socket().exists());
    let h = w.open();
    wait_for("the socket again", || {
        h.status().mcp_listening.then_some(())
    });
}

#[test]
fn an_agent_over_the_socket_sees_tokens_only_and_its_read_reaches_the_footer() {
    let w = World::new();
    let h = w.open();
    h.ingest(vec![w.download("w2-acme-2025.txt")], IngestMode::Move)
        .unwrap();
    let q = wait_queue(&h, |q| docs(q).iter().any(|d| d.state == DocState::Ready));
    let codename = q[0].codename.clone();
    wait_for("the socket", || h.status().mcp_listening.then_some(()));
    let n = counter(&h);

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let (listed, resolved) = rt.block_on(async {
        let stream = tokio::net::UnixStream::connect(w.socket()).await.unwrap();
        let mut c = McpClient::connect(stream, "inker-test").await.unwrap();
        // The footer's "connected" state: the client is listed while its
        // session is open, under its name.
        wait_for("the session", || {
            (h.status().clients == vec!["inker-test".to_string()]).then_some(())
        });
        let listed = c.call_tool("list", json!({})).await.unwrap();
        let resolved = c
            .call_tool("resolve", json!({"handle": codename}))
            .await
            .unwrap();
        (listed, resolved)
    });
    assert!(!listed.is_error, "{}", listed.text);
    assert!(listed.text.contains(&codename), "{}", listed.text);
    assert!(!resolved.is_error, "{}", resolved.text);
    assert!(resolved.text.contains("[SSN_"), "{}", resolved.text);
    assert!(
        !resolved.text.contains(JORDAN_SSN),
        "a real value reached the agent"
    );
    wait_for("the session to end", || {
        h.status().clients.is_empty().then_some(())
    });
    if let Some(v) = &resolved.structured {
        assert!(
            !v.to_string().contains(JORDAN_SSN),
            "a real value reached the agent"
        );
    }

    let reads = wait_for("the read to reach the footer", || {
        let r = h.recent_reads(5);
        (!r.is_empty()).then_some(r)
    });
    assert_eq!(reads[0].agent, "inker-test");
    assert_eq!(reads[0].codename, codename);
    wait_for("the sink after a read", || {
        (n.load(Ordering::SeqCst) >= 1).then_some(())
    });
}

// ---------------------------------------------------------------------------
// Finding documents over the real core
// ---------------------------------------------------------------------------

use inker_ffi::{BoardEntry, MatchField};

fn ready_doc(entry: &BoardEntry) -> &DocItem {
    match entry {
        BoardEntry::Doc { item } => item,
        BoardEntry::Folder { .. } => panic!("a folder"),
    }
}

#[test]
fn the_board_carries_the_payer_the_tag_and_the_year_and_find_reads_the_inked_text() {
    let w = World::new();
    let h = w.open();
    h.ingest(vec![w.download("w2-acme-2025.txt")], IngestMode::Move)
        .unwrap();
    wait_queue(&h, all_settled);
    let b = h.board(None);
    assert_eq!(b.total, 1);
    assert_eq!(b.groups.len(), 1);
    assert_eq!(b.groups[0].year, Some(2025));
    let doc = ready_doc(&b.groups[0].entries[0]);
    assert_eq!(doc.kind_label, "W-2");
    assert_eq!(doc.tag.as_deref(), Some("Taxes"));
    assert_eq!(doc.year, Some(2025));
    assert_eq!(
        doc.payer.as_deref(),
        Some("Acme Robotics Inc"),
        "the template's employer, read back through the vault"
    );
    assert_eq!(b.tags[0].tag, "Taxes");
    assert_eq!(b.tags[0].count, 1);

    let hits = h.find("wage and tax".into());
    assert_eq!(hits.len(), 1, "{hits:#?}");
    assert_eq!(hits[0].field, MatchField::Text);
    assert!(
        hits[0]
            .snippet
            .as_deref()
            .unwrap()
            .to_lowercase()
            .contains("wage and tax")
    );
    let hits = h.find("robotics".into());
    assert_eq!(hits[0].field, MatchField::Payer, "{hits:#?}");
    assert_eq!(hits[0].matched, "Robotics");
    assert!(
        h.find(JORDAN_SSN.into()).is_empty(),
        "a real value never matches"
    );
    let found = h.locate(format!("🐙/{}", doc.codename)).unwrap();
    assert_eq!(ready_doc(&found).id, doc.id);
    let d = h.details(doc.id.clone()).unwrap();
    assert_eq!(d.location, "");
    assert_eq!(d.dropped_with, 0);
    assert!(d.last_read.is_none());
}

#[test]
fn forget_leaves_the_file_and_ink_again_keeps_the_handle() {
    let w = World::new();
    let h = w.open();
    h.ingest(vec![w.download("w2-acme-2025.txt")], IngestMode::Move)
        .unwrap();
    let q = wait_queue(&h, all_settled);
    let doc = docs(&q)[0].clone();
    let n = counter(&h);

    h.ink_again(doc.id.clone()).unwrap();
    let q = wait_queue(&h, |q| all_settled(q) && n.load(Ordering::SeqCst) >= 2);
    let again = docs(&q)[0];
    assert_eq!(again.codename, doc.codename, "the handle survives");
    assert_eq!(again.state, DocState::Ready);

    h.forget(doc.id.clone()).unwrap();
    let q = wait_queue(&h, |q| q.is_empty());
    assert!(q.is_empty());
    assert!(
        Path::new(&doc.path).exists(),
        "the original stays in the private folder"
    );
    assert!(h.board(None).groups.is_empty());
    assert!(matches!(h.forget(doc.id), Err(FfiError::NotFound { .. })));
}

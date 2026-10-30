//! `HostBackend`: the FFI [`Backend`] over the real core (`inker_host::Host`),
//! plus the MCP socket the app process serves (stack-decision section 3.2).
//!
//! Threads:
//! - the caller (Swift's main thread) files drops and answers questions;
//! - one worker processes documents in FIFO order and refreshes the queue;
//! - one tokio runtime serves the socket.
//!
//! `Host` holds the core behind one lock, and processing a document holds it
//! for seconds. So `queue` and `recent_reads` never take that lock: they read
//! a snapshot that is rebuilt after every state change and then announced
//! through the change sink.

use std::os::unix::fs::FileTypeExt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::{Duration, SystemTime};

use camino::Utf8PathBuf;
use chrono::{DateTime, Utc};
use inker_core::facade::OUTPUT_KIND;
use inker_core::find::{self, DocView};
use inker_core::{
    Core, Description, DocumentId, Error, FolderId, Handle, ListEntry, QuestionId,
    Resolved as CoreResolved,
};
use inker_extract::{FileKind, detect_file_kind};
use inker_host::Host;
use inker_mcp::{Resolved, ServerOptions, WriteReceipt};
use tokio_util::sync::CancellationToken;

use crate::backend::{Backend, ChangeSink};
use crate::error::FfiError;
use crate::types::{
    Board, BoardEntry, DocDetails, DocItem, DocState, FindMatch, FolderItem, FolderSummary, Loader,
    QueueItem, ReadEvent, ReviewAnswer, ReviewQuestion, Status, TagCount, TextSpan, YearGroup,
};

/// How many agent reads the snapshot keeps for the footer.
const READS_KEPT: usize = 20;
/// How long `close` waits for the socket's sessions to wind down.
const SHUTDOWN_GRACE: Duration = Duration::from_secs(3);

enum Job {
    /// Process these documents, in order.
    Process(Vec<DocumentId>),
    /// Something changed outside the worker (an agent read or wrote).
    Refresh,
}

#[derive(Debug, Default)]
struct Snapshot {
    queue: Vec<QueueItem>,
    reads: Vec<ReadEvent>,
    board: Board,
}

/// State the worker, the socket and the handle share.
#[derive(Debug)]
struct Shared {
    sink: Mutex<ChangeSink>,
    snapshot: Mutex<Snapshot>,
    closing: AtomicBool,
    server_alive: AtomicBool,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

impl Shared {
    /// Rebuild the snapshot from the core, then tell the shell. Returns the
    /// documents still `Queued`.
    fn refresh(&self, host: &Host) -> Vec<DocumentId> {
        let built = {
            let core = host.core();
            collect(&core).and_then(|(queue, queued, board)| {
                let reads = core.recent_reads(READS_KEPT)?;
                Ok((queue, queued, board, reads))
            })
        };
        let queued = match built {
            Ok((queue, queued, board, reads)) => {
                let mut snap = lock(&self.snapshot);
                snap.queue = queue;
                snap.board = board;
                snap.reads = reads.into_iter().map(read_event).collect();
                queued
            }
            Err(e) => {
                tracing::warn!(kind = error_kind(&e), "could not rebuild the queue");
                Vec::new()
            }
        };
        let sink = lock(&self.sink).clone();
        sink.notify();
        queued
    }
}

/// The socket server: a tokio runtime on its own thread.
#[derive(Debug)]
struct SocketServer {
    token: CancellationToken,
    thread: JoinHandle<()>,
    path: PathBuf,
}

/// See the module docs.
pub struct HostBackend {
    host: Arc<Host>,
    shared: Arc<Shared>,
    private_folder_display: String,
    data_dir: Utf8PathBuf,
    jobs: Mutex<Option<Sender<Job>>>,
    worker: Mutex<Option<JoinHandle<()>>>,
    server: Mutex<Option<SocketServer>>,
}

impl std::fmt::Debug for HostBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HostBackend")
            .field("private_folder_display", &self.private_folder_display)
            .finish_non_exhaustive()
    }
}

impl HostBackend {
    /// Wrap an open host and start the worker. Documents a previous run left
    /// `Queued`, or part way through (`Reading`, `Inking`: it died), are
    /// processed straight away.
    pub fn new(host: Host, private_folder_display: &str) -> Result<Arc<Self>, FfiError> {
        let host = Arc::new(host);
        let data_dir = host.core().paths().data_dir.clone();
        let shared = Arc::new(Shared {
            sink: Mutex::new(ChangeSink::noop()),
            snapshot: Mutex::new(Snapshot::default()),
            closing: AtomicBool::new(false),
            server_alive: AtomicBool::new(false),
        });
        let (tx, rx) = mpsc::channel();
        let worker = {
            let (host, shared) = (host.clone(), shared.clone());
            thread::Builder::new()
                .name("inker-ingest".into())
                .spawn(move || run_worker(&host, &shared, rx))
                .map_err(|_| backend_err("could not start the ingest worker"))?
        };
        match inker_pipeline::recover(&host.core()) {
            Ok(ids) if !ids.is_empty() => {
                tracing::info!(count = ids.len(), "requeued interrupted documents");
            }
            Ok(_) => {}
            Err(e) => tracing::warn!(
                kind = error_kind(&e),
                "could not requeue interrupted documents"
            ),
        }
        let leftover = shared.refresh(&host);
        if !leftover.is_empty() {
            tracing::info!(count = leftover.len(), "resuming queued documents");
            let _ = tx.send(Job::Process(leftover));
        }
        Ok(Arc::new(Self {
            host,
            shared,
            private_folder_display: private_folder_display.to_string(),
            data_dir,
            jobs: Mutex::new(Some(tx)),
            worker: Mutex::new(Some(worker)),
            server: Mutex::new(None),
        }))
    }

    fn send(&self, job: Job) {
        if let Some(tx) = lock(&self.jobs).as_ref() {
            let _ = tx.send(job);
        }
    }

    /// Serve MCP on `socket_path` until [`HostBackend::close`]. A bind failure
    /// (another INKER is listening) is logged and shows as "not listening";
    /// the queue keeps working.
    pub fn serve_socket(&self, socket_path: PathBuf) -> Result<(), FfiError> {
        let Some(jobs) = lock(&self.jobs).clone() else {
            return Err(backend_err("closed"));
        };
        let mut slot = lock(&self.server);
        if slot.is_some() {
            return Ok(());
        }
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("inker-mcp")
            .enable_all()
            .build()
            .map_err(|_| backend_err("could not start the socket runtime"))?;
        let bridge: Arc<dyn inker_mcp::Backend> = Arc::new(McpBridge {
            host: self.host.clone(),
            jobs: Mutex::new(jobs),
        });
        let token = CancellationToken::new();
        let shared = self.shared.clone();
        shared.server_alive.store(true, Ordering::SeqCst);
        let (path, t) = (socket_path.clone(), token.clone());
        let thread = thread::Builder::new()
            .name("inker-socket".into())
            .spawn(move || {
                let served = runtime.block_on(inker_ipc::host::serve(
                    &path,
                    bridge,
                    ServerOptions::default(),
                    t,
                ));
                if let Err(e) = served {
                    tracing::warn!(error = %e, "socket server stopped");
                }
                shared.server_alive.store(false, Ordering::SeqCst);
                runtime.shutdown_timeout(SHUTDOWN_GRACE);
            });
        match thread {
            Ok(thread) => {
                *slot = Some(SocketServer {
                    token,
                    thread,
                    path: socket_path,
                });
                Ok(())
            }
            Err(_) => {
                self.shared.server_alive.store(false, Ordering::SeqCst);
                Err(backend_err("could not start the socket thread"))
            }
        }
    }

    fn listening(&self) -> bool {
        if !self.shared.server_alive.load(Ordering::SeqCst) {
            return false;
        }
        lock(&self.server).as_ref().is_some_and(|s| {
            std::fs::symlink_metadata(&s.path).is_ok_and(|m| m.file_type().is_socket())
        })
    }

    /// Stop the socket, then the worker. Documents not yet processed stay
    /// `Queued` and are picked up by the next open. Idempotent.
    pub fn shutdown(&self) {
        self.shared.closing.store(true, Ordering::SeqCst);
        if let Some(server) = lock(&self.server).take() {
            server.token.cancel();
            join(server.thread);
        }
        drop(lock(&self.jobs).take());
        if let Some(worker) = lock(&self.worker).take() {
            join(worker);
        }
    }
}

/// Join unless that would be the thread joining itself (the last handle
/// dropped from inside a callback).
fn join(handle: JoinHandle<()>) {
    if handle.thread().id() != thread::current().id() && handle.join().is_err() {
        tracing::error!("a background thread panicked");
    }
}

impl Drop for HostBackend {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn run_worker(host: &Host, shared: &Shared, rx: Receiver<Job>) {
    for job in rx {
        match job {
            Job::Refresh => {
                shared.refresh(host);
            }
            Job::Process(ids) => {
                for id in ids {
                    if shared.closing.load(Ordering::SeqCst) {
                        return;
                    }
                    mark_reading(host, id);
                    shared.refresh(host);
                    let outcome = host.process(id);
                    tracing::debug!(state = ?outcome.state, "processed");
                    shared.refresh(host);
                }
            }
        }
    }
}

/// Show the row as reading before the core takes its lock for the whole
/// extraction. The pipeline sets the same state and note itself.
fn mark_reading(host: &Host, id: DocumentId) {
    let core = host.core();
    let Ok(rec) = core.document(id) else { return };
    if rec.state != inker_core::DocState::Queued || rec.kind.as_str() == OUTPUT_KIND {
        return;
    }
    let note = match detect_file_kind(&rec.path) {
        FileKind::Image => NOTE_SCAN,
        _ => NOTE_READING,
    };
    if let Err(e) = core.set_state(id, inker_core::DocState::Reading, Some(note)) {
        tracing::warn!(kind = error_kind(&e), "could not mark reading");
    }
}

const NOTE_READING: &str = "reading";
const NOTE_SCAN: &str = "reading the scan";
const NOTE_QUEUED: &str = "queued";

impl Backend for HostBackend {
    fn set_change_sink(&self, sink: ChangeSink) {
        *lock(&self.shared.sink) = sink;
    }

    fn ingest(
        &self,
        paths: Vec<Utf8PathBuf>,
        mode: inker_core::types::IngestMode,
    ) -> Result<(), FfiError> {
        if paths.is_empty() {
            return Err(FfiError::InvalidArgument {
                reason: "nothing to ingest".into(),
            });
        }
        // The message never names the file: file names can be personal.
        if paths.iter().any(|p| !p.exists()) {
            return Err(FfiError::InvalidArgument {
                reason: "a dropped file no longer exists".into(),
            });
        }
        // One path at a time, so a failure part way still processes what
        // was already moved in.
        let mut ids = Vec::new();
        let mut failed = None;
        {
            let core = self.host.core();
            for p in &paths {
                match core.begin_ingest(std::slice::from_ref(p), mode) {
                    Ok(mut new) => ids.append(&mut new),
                    Err(e) => {
                        failed = Some(e);
                        break;
                    }
                }
            }
        }
        self.shared.refresh(&self.host);
        if !ids.is_empty() {
            self.send(Job::Process(ids));
        }
        match failed {
            Some(e) => Err(core_error(&e)),
            None => Ok(()),
        }
    }

    fn queue(&self) -> Vec<QueueItem> {
        lock(&self.shared.snapshot).queue.clone()
    }

    fn answer(&self, question_id: &str, answer: ReviewAnswer) -> Result<(), FfiError> {
        let id =
            question_id
                .parse::<i64>()
                .map(QuestionId)
                .map_err(|_| FfiError::InvalidArgument {
                    reason: "malformed question id".into(),
                })?;
        let (doc, state) = {
            let core = self.host.core();
            let doc = core
                .pending_reviews()
                .map_err(|e| core_error(&e))?
                .into_iter()
                .find(|q| q.id == id)
                .map(|q| q.document)
                .ok_or_else(|| FfiError::NotFound {
                    reason: "no open question with that id".into(),
                })?;
            let state = core
                .answer_review(id, answer.into())
                .map_err(|e| core_error(&e))?;
            (doc, state)
        };
        if state == inker_core::DocState::Queued {
            self.send(Job::Process(vec![doc]));
        }
        self.shared.refresh(&self.host);
        Ok(())
    }

    fn recent_reads(&self, n: u32) -> Vec<ReadEvent> {
        let snap = lock(&self.shared.snapshot);
        snap.reads.iter().take(n as usize).cloned().collect()
    }

    fn status(&self) -> Status {
        Status {
            mcp_listening: self.listening(),
            private_folder_display: self.private_folder_display.clone(),
        }
    }

    fn private_folder(&self) -> Option<String> {
        Some(self.host.core().paths().private_folder.to_string())
    }

    fn board(&self) -> Board {
        lock(&self.shared.snapshot).board.clone()
    }

    fn find(&self, query: &str) -> Vec<FindMatch> {
        let core = self.host.core();
        let found = core
            .find(query)
            .and_then(|m| Ok((m, core.pending_reviews()?)));
        match found {
            Ok((matches, pending)) => matches
                .into_iter()
                .map(|m| FindMatch {
                    doc: doc_item(&m.doc, &pending),
                    field: m.field.into(),
                    matched: m.matched,
                    snippet: m.snippet,
                    exact_handle: m.exact_handle,
                })
                .collect(),
            Err(e) => {
                tracing::warn!(kind = error_kind(&e), "find failed");
                Vec::new()
            }
        }
    }

    fn locate(&self, codename: &str) -> Option<BoardEntry> {
        let snap = lock(&self.shared.snapshot);
        snap.board
            .entries()
            .find(|e| e.codename() == codename)
            .cloned()
    }

    fn details(&self, id: &str) -> Result<DocDetails, FfiError> {
        let id = DocumentId::parse(id).ok_or_else(|| FfiError::NotFound {
            reason: "no document with that id".into(),
        })?;
        let core = self.host.core();
        let d = core.details(id).map_err(|e| core_error(&e))?;
        Ok(DocDetails {
            location: d.location,
            dropped_at: SystemTime::from(d.dropped_at),
            dropped_with: d.dropped_with,
            last_read: d.last_read.map(read_event),
        })
    }

    fn forget(&self, id: &str) -> Result<(), FfiError> {
        let (doc, folder) = parse_ids(id)?;
        {
            let core = self.host.core();
            if core.document(doc).is_ok() {
                core.forget(doc).map_err(|e| core_error(&e))?;
            } else {
                core.forget_folder(folder).map_err(|e| core_error(&e))?;
            }
        }
        self.shared.refresh(&self.host);
        Ok(())
    }

    fn ink_again(&self, id: &str) -> Result<(), FfiError> {
        let (doc, folder) = parse_ids(id)?;
        let ids = {
            let core = self.host.core();
            if core.document(doc).is_ok() {
                core.ink_again(doc).map_err(|e| core_error(&e))?;
                vec![doc]
            } else {
                core.ink_again_folder(folder).map_err(|e| core_error(&e))?
            }
        };
        self.shared.refresh(&self.host);
        if !ids.is_empty() {
            self.send(Job::Process(ids));
        }
        Ok(())
    }

    fn doctor(&self) -> Vec<String> {
        let socket = lock(&self.server).as_ref().map(|s| s.path.clone());
        let mcp = match socket {
            Some(path) if self.listening() => format!("mcp: listening at {}", path.display()),
            _ => "mcp: not listening".to_string(),
        };
        vec![
            mcp,
            format!("data dir: {}", self.data_dir),
            format!("private folder: {}", self.private_folder_display),
            crate::agents::AGENTS_NOT_CHECKED.to_string(),
        ]
    }

    fn close(&self) {
        self.shutdown();
    }
}

/// The MCP backend the socket serves: the host, plus a nudge to the worker
/// whenever an agent changes what the popover shows.
struct McpBridge {
    host: Arc<Host>,
    jobs: Mutex<Sender<Job>>,
}

impl McpBridge {
    fn nudge(&self) {
        let _ = lock(&self.jobs).send(Job::Refresh);
    }
}

impl inker_mcp::Backend for McpBridge {
    fn list(&self) -> inker_core::Result<Vec<ListEntry>> {
        self.host.backend().list()
    }

    fn resolve(&self, h: &Handle) -> inker_core::Result<Resolved> {
        self.host.backend().resolve(h)
    }

    fn describe(&self, h: &Handle) -> inker_core::Result<Description> {
        self.host.backend().describe(h)
    }

    fn write(&self, h: &Handle, name: &str, content: &str) -> inker_core::Result<WriteReceipt> {
        let out = self.host.backend().write(h, name, content);
        self.nudge();
        out
    }

    fn note_read(&self, agent: &str, h: &Handle) {
        self.host.backend().note_read(agent, h);
        self.nudge();
    }

    fn pending_reviews(&self) -> inker_core::Result<Vec<inker_mcp::PendingReview>> {
        self.host.backend().pending_reviews()
    }

    fn answer_review(
        &self,
        id: QuestionId,
        answer: inker_core::ReviewAnswer,
    ) -> inker_core::Result<inker_core::DocState> {
        let out = self.host.backend().answer_review(id, answer);
        self.nudge();
        out
    }

    fn status(&self) -> inker_core::Result<inker_mcp::ServerStatus> {
        self.host.backend().status()
    }
}

// ---------------------------------------------------------------------------
// Core records -> FFI records
// ---------------------------------------------------------------------------

/// The queue, newest drop first, and the ids still `Queued`.
/// A core `DocumentId`'s string form, as both a document and a folder id:
/// the shell passes either, and the backend tries the document first.
fn parse_ids(id: &str) -> Result<(DocumentId, FolderId), FfiError> {
    match (DocumentId::parse(id), FolderId::parse(id)) {
        (Some(d), Some(f)) => Ok((d, f)),
        _ => Err(FfiError::NotFound {
            reason: "no document or folder with that id".into(),
        }),
    }
}

type Collected = (Vec<QueueItem>, Vec<DocumentId>, Board);

fn collect(core: &Core) -> inker_core::Result<Collected> {
    let pending = core.pending_reviews()?;
    let (views, _) = core.doc_views()?;
    let by_id: std::collections::HashMap<DocumentId, &DocView> =
        views.iter().map(|v| (v.record.id, v)).collect();
    let board = board_item(core.board(None)?, &pending);
    let mut groups: Vec<(Option<DateTime<Utc>>, QueueItem)> = Vec::new();
    let mut queued = Vec::new();
    for entry in core.list()? {
        let recs: Vec<&DocView> = if entry.is_folder {
            let CoreResolved::Folder(f) = core.resolve(&entry.handle)? else {
                continue;
            };
            let mut recs: Vec<&DocView> = f
                .children
                .iter()
                .filter_map(|c| core.find_document(&c.handle).ok())
                .filter_map(|r| by_id.get(&r.id).copied())
                .collect();
            recs.sort_by(|a, b| {
                (a.record.created_at, &a.record.original_name)
                    .cmp(&(b.record.created_at, &b.record.original_name))
            });
            recs
        } else {
            let r = core.find_document(&entry.handle)?;
            by_id.get(&r.id).copied().into_iter().collect()
        };
        queued.extend(
            recs.iter()
                .filter(|v| v.record.state == inker_core::DocState::Queued)
                .map(|v| v.record.id),
        );
        let newest = recs.iter().map(|v| v.record.created_at).max();
        let children: Vec<DocItem> = recs.iter().map(|v| doc_item(v, &pending)).collect();
        let item = if entry.is_folder {
            let folder = recs.iter().find_map(|v| v.record.folder);
            let summary = match folder {
                Some(id) => core.folder_summary(id)?.into(),
                None => FolderSummary::of(&children),
            };
            QueueItem {
                id: folder.map_or_else(
                    || format!("folder:{}", entry.handle.codename.as_str()),
                    |f| f.0.to_string(),
                ),
                codename: entry.handle.codename.as_str().to_string(),
                is_folder: true,
                summary,
                children,
            }
        } else {
            let Some(first) = recs.first() else { continue };
            QueueItem {
                id: first.record.id.0.to_string(),
                codename: entry.handle.codename.as_str().to_string(),
                is_folder: false,
                summary: FolderSummary::of(&children),
                children,
            }
        };
        groups.push((newest, item));
    }
    // Newest first; an empty folder (no time) last; ties by codename.
    groups.sort_by(|(ta, a), (tb, b)| tb.cmp(ta).then_with(|| a.codename.cmp(&b.codename)));
    Ok((groups.into_iter().map(|(_, i)| i).collect(), queued, board))
}

/// The core's board as the shell sees it.
fn board_item(b: find::Board, pending: &[inker_core::ReviewQuestion]) -> Board {
    let entry = |e: find::BoardEntry| match e {
        find::BoardEntry::Doc(v) => BoardEntry::Doc {
            item: doc_item(&v, pending),
        },
        find::BoardEntry::Folder(f) => BoardEntry::Folder {
            item: FolderItem {
                id: f.record.id.0.to_string(),
                codename: f.record.codename.as_str().to_string(),
                name: f.name,
                path: f.record.path.to_string(),
                summary: f.summary.into(),
                children: f.children.iter().map(|c| doc_item(c, pending)).collect(),
            },
        },
    };
    Board {
        groups: b
            .groups
            .into_iter()
            .map(|g| YearGroup {
                year: g.year,
                files: g.files,
                entries: g.entries.into_iter().map(entry).collect(),
            })
            .collect(),
        tags: b
            .tags
            .into_iter()
            .map(|t| TagCount {
                tag: t.tag,
                count: t.count,
            })
            .collect(),
        total: b.total,
    }
}

fn doc_item(view: &DocView, pending: &[inker_core::ReviewQuestion]) -> DocItem {
    let rec = &view.record;
    let state: DocState = rec.state.into();
    let question = if state == DocState::NeedsAttention {
        pending
            .iter()
            .filter(|q| q.document == rec.id)
            .min_by_key(|q| q.id.0)
            .map(review_question)
    } else {
        None
    };
    let status_note = rec.status_note.clone().or_else(|| match state {
        DocState::Queued => Some(NOTE_QUEUED.to_string()),
        DocState::NeedsAttention => question.as_ref().map(|q| q.location.clone()),
        _ => None,
    });
    DocItem {
        id: rec.id.0.to_string(),
        codename: rec.codename.as_str().to_string(),
        original_name: rec.original_name.clone(),
        path: rec.path.to_string(),
        kind_label: find::kind_label(&rec.kind),
        state,
        inked: rec.inked,
        loader: loader_for(state, status_note.as_deref()),
        status_note,
        question,
        year: rec.year,
        payer: view.payer.clone(),
        tag: rec.tag.clone(),
        dropped_at: SystemTime::from(rec.created_at),
        folder_name: view.folder_name.clone(),
    }
}

/// The attention card. The core keeps the value; the card gets the prompt,
/// where to look, the words just before the value and the value's length.
fn review_question(q: &inker_core::ReviewQuestion) -> ReviewQuestion {
    let location = if q.page_count > 0 {
        format!("page {} of {}", q.page, q.page_count)
    } else {
        format!("page {}", q.page)
    };
    ReviewQuestion {
        id: q.id.0.to_string(),
        prompt: q
            .prompt
            .clone()
            .unwrap_or_else(|| DEFAULT_PROMPT.to_string()),
        location,
        page_index: q.page.saturating_sub(1),
        // The peek draws its own gap before the outline.
        context_before: q.context_before.trim_end().to_string(),
        value_length: q.value_length,
        span: q.span.map(|s| TextSpan {
            start: u32::try_from(s.start).unwrap_or(u32::MAX),
            end: u32::try_from(s.end).unwrap_or(u32::MAX),
        }),
    }
}

const DEFAULT_PROMPT: &str = "Ink it?";

fn loader_for(state: DocState, note: Option<&str>) -> Loader {
    match state {
        DocState::Queued => Loader::Queued,
        DocState::Reading if note == Some(NOTE_SCAN) => Loader::Ocr,
        DocState::Reading | DocState::Inking => Loader::Read,
        DocState::NeedsAttention | DocState::Ready | DocState::Failed => Loader::Idle,
    }
}

fn read_event(r: inker_core::AgentRead) -> ReadEvent {
    ReadEvent {
        agent: r.agent,
        codename: r.handle.codename.as_str().to_string(),
        at: SystemTime::from(r.at),
    }
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

fn backend_err(reason: &str) -> FfiError {
    FfiError::Backend {
        reason: reason.into(),
    }
}

/// A core error as Swift sees it. Core messages can carry paths and file
/// names, and Swift logs these reasons publicly, so only the kind crosses.
pub(crate) fn core_error(e: &Error) -> FfiError {
    let reason = format!("{} error", error_kind(e));
    match e {
        Error::UnknownHandle(_) => FfiError::NotFound { reason },
        Error::InvalidHandle(_) => FfiError::InvalidArgument { reason },
        _ => FfiError::Backend { reason },
    }
}

/// A short, content-free label for logs and FFI reasons.
fn error_kind(e: &Error) -> &'static str {
    match e {
        Error::InvalidHandle(_) => "invalid handle",
        Error::UnknownHandle(_) => "unknown handle",
        Error::PendingReview(_) => "pending review",
        Error::NotReady(_) => "not ready",
        Error::Vault(_) => "vault",
        Error::Index(_) => "index",
        Error::Extract(_) => "extract",
        Error::Detect(_) => "detect",
        Error::Ingest(_) => "ingest",
        Error::Refused(_) => "refused",
        Error::Io(_) => "io",
        Error::Json(_) => "json",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn core_errors_cross_as_their_kind_only() {
        let io = Error::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "/Users/someone/Downloads/secret-name.pdf",
        ));
        let e = core_error(&io);
        assert!(matches!(e, FfiError::Backend { .. }), "{e:?}");
        assert!(!e.to_string().contains("secret-name"), "{e}");
        assert!(e.to_string().contains("io"), "{e}");

        let unknown = core_error(&Error::UnknownHandle("w2-x".into()));
        assert!(matches!(unknown, FfiError::NotFound { .. }), "{unknown:?}");
        let invalid = core_error(&Error::InvalidHandle("bad".into()));
        assert!(
            matches!(invalid, FfiError::InvalidArgument { .. }),
            "{invalid:?}"
        );
        let vault = core_error(&Error::Vault("key".into()));
        assert!(matches!(vault, FfiError::Backend { .. }));
    }

    #[test]
    fn loaders_follow_state_and_note() {
        assert_eq!(loader_for(DocState::Queued, None), Loader::Queued);
        assert_eq!(loader_for(DocState::Reading, Some(NOTE_SCAN)), Loader::Ocr);
        assert_eq!(
            loader_for(DocState::Reading, Some(NOTE_READING)),
            Loader::Read
        );
        assert_eq!(loader_for(DocState::Inking, None), Loader::Read);
        assert_eq!(loader_for(DocState::Ready, None), Loader::Idle);
        assert_eq!(loader_for(DocState::NeedsAttention, None), Loader::Idle);
        assert_eq!(loader_for(DocState::Failed, None), Loader::Idle);
    }
}

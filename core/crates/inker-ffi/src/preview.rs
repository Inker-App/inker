//! `PreviewBackend`: fake data that drives the four popover states, so the app
//! runs end to end with no core wired.
//!
//! It never moves, copies, opens or reads the contents of a dropped file. It
//! only lists the entries of a dropped folder to make rows. All names and
//! counts in the fixed scenarios are invented.

use std::collections::HashSet;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, SystemTime};

use camino::{Utf8Path, Utf8PathBuf};

use crate::backend::{Backend, ChangeSink};
use crate::error::FfiError;
use crate::types::{
    DocItem, DocState, FolderSummary, Loader, QueueItem, ReadEvent, ReviewAnswer, ReviewQuestion,
    Status,
};

/// The four popover boards.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum PreviewScenario {
    /// `AppPopEmpty`
    Empty,
    /// `AppPopProcessing`
    Processing,
    /// `AppPopAttention`
    Attention,
    /// `AppPopReady`
    Ready,
}

#[derive(Debug, Clone)]
struct Group {
    id: String,
    codename: String,
    is_folder: bool,
    docs: Vec<DocItem>,
}

impl Group {
    fn to_item(&self) -> QueueItem {
        QueueItem {
            id: self.id.clone(),
            codename: self.codename.clone(),
            is_folder: self.is_folder,
            summary: FolderSummary::of(&self.docs),
            children: self.docs.clone(),
        }
    }
}

#[derive(Debug)]
struct Inner {
    /// Newest first.
    groups: Vec<Group>,
    reads: Vec<ReadEvent>,
    sink: ChangeSink,
    next_id: u64,
}

impl Inner {
    fn fresh_id(&mut self, prefix: &str) -> String {
        self.next_id += 1;
        format!("{prefix}-{}", self.next_id)
    }

    fn taken(&self) -> HashSet<String> {
        let mut s = HashSet::new();
        for g in &self.groups {
            s.insert(g.codename.clone());
            for d in &g.docs {
                s.insert(d.codename.clone());
            }
        }
        s
    }
}

/// See the module docs.
#[derive(Debug, Clone)]
pub struct PreviewBackend {
    inner: Arc<Mutex<Inner>>,
    private_folder_display: String,
    tick: Duration,
}

/// Simulated step length for scenario backends that also get drops.
const SCENARIO_TICK: Duration = Duration::from_millis(700);

impl PreviewBackend {
    /// Starts empty; drops are simulated, one pipeline step per `tick`.
    pub fn live(private_folder_display: &str, tick: Duration) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                groups: Vec::new(),
                reads: Vec::new(),
                sink: ChangeSink::noop(),
                next_id: 0,
            })),
            private_folder_display: private_folder_display.to_string(),
            tick,
        }
    }

    /// Frozen in one of the four boards. Answers and new drops still work.
    pub fn scenario(scenario: PreviewScenario) -> Self {
        let b = Self::live("~/Private", SCENARIO_TICK);
        {
            let mut inner = b.lock();
            let (groups, reads) = fixture(scenario);
            inner.groups = groups;
            inner.reads = reads;
            inner.next_id = 100;
        }
        b
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        // A poisoned lock means a panic mid-update; the data is still usable
        // for a preview, so recover rather than propagate.
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Apply `f` to the doc, then notify outside the lock (the listener may
    /// call back into `queue`).
    fn update_doc(
        inner: &Arc<Mutex<Inner>>,
        group_id: &str,
        doc_id: &str,
        f: impl FnOnce(&mut DocItem),
    ) {
        let sink = {
            let mut g = inner.lock().unwrap_or_else(|e| e.into_inner());
            let found = g
                .groups
                .iter_mut()
                .find(|gr| gr.id == group_id)
                .and_then(|gr| gr.docs.iter_mut().find(|d| d.id == doc_id));
            match found {
                Some(d) => f(d),
                None => return,
            }
            g.sink.clone()
        };
        sink.notify();
    }

    /// Walk each doc of a group through reading, inking and done.
    fn simulate(
        inner: Arc<Mutex<Inner>>,
        group_id: String,
        docs: Vec<(String, bool, bool, u32)>,
        tick: Duration,
    ) {
        std::thread::spawn(move || {
            for (doc_id, scan, hold, inked) in docs {
                std::thread::sleep(tick);
                Self::update_doc(&inner, &group_id, &doc_id, |d| {
                    d.state = DocState::Reading;
                    d.status_note = Some(if scan { "reading the scan" } else { "reading" }.into());
                    d.loader = if scan { Loader::Ocr } else { Loader::Read };
                });
                std::thread::sleep(tick);
                Self::update_doc(&inner, &group_id, &doc_id, |d| {
                    d.state = DocState::Inking;
                    d.status_note = Some("inking".into());
                    d.loader = Loader::Read;
                    d.inked = inked / 2;
                });
                std::thread::sleep(tick);
                Self::update_doc(&inner, &group_id, &doc_id, |d| {
                    d.loader = Loader::Idle;
                    d.inked = inked;
                    if hold {
                        d.state = DocState::NeedsAttention;
                        d.status_note = Some("page 3 of 11".into());
                        d.question = Some(sample_question(&d.id));
                    } else {
                        d.state = DocState::Ready;
                        d.status_note = Some("ready".into());
                    }
                });
            }
        });
    }
}

impl Backend for PreviewBackend {
    fn set_change_sink(&self, sink: ChangeSink) {
        self.lock().sink = sink;
    }

    fn ingest(
        &self,
        paths: Vec<Utf8PathBuf>,
        _mode: inker_core::types::IngestMode,
    ) -> Result<(), FfiError> {
        if paths.is_empty() {
            return Err(FfiError::InvalidArgument {
                reason: "nothing to ingest".into(),
            });
        }
        if let Some(missing) = paths.iter().find(|p| !p.exists()) {
            return Err(FfiError::InvalidArgument {
                reason: format!("no such file: {missing}"),
            });
        }

        // Each dropped folder becomes a folder item; loose files each become
        // their own item, matching the core's folder model.
        let mut planned: Vec<(String, bool, Vec<Utf8PathBuf>)> = Vec::new();
        for p in &paths {
            let name = p.file_name().unwrap_or("document").to_string();
            if p.is_dir() {
                let files = list_files(p).into_iter().map(|f| p.join(f)).collect();
                planned.push((name, true, files));
            } else {
                planned.push((name, false, vec![p.clone()]));
            }
        }

        let mut work = Vec::new();
        let sink = {
            let mut inner = self.lock();
            for (name, is_folder, files) in planned {
                let mut taken = inner.taken();
                let base = if is_folder {
                    slug(&name)
                } else {
                    codename_from_name(&name)
                };
                let codename = unique(&base, &taken);
                taken.insert(codename.clone());
                let group_id = inner.fresh_id("g");
                let mut docs = Vec::new();
                let mut sim = Vec::new();
                for (i, file_path) in files.iter().enumerate() {
                    let file = file_path.file_name().unwrap_or("document");
                    let doc_codename = if is_folder {
                        let c = unique(&codename_from_name(file), &taken);
                        taken.insert(c.clone());
                        c
                    } else {
                        codename.clone()
                    };
                    let id = inner.fresh_id("d");
                    let scan = is_scan(file);
                    // One in four is held, so a folder drop shows the card.
                    let hold = is_folder && i % 4 == 3;
                    sim.push((id.clone(), scan, hold, inked_for(file)));
                    let mut d = doc(
                        &id,
                        &doc_codename,
                        file,
                        DocState::Queued,
                        0,
                        "queued",
                        Loader::Queued,
                    );
                    d.path = file_path.to_string();
                    docs.push(d);
                }
                inner.groups.insert(
                    0,
                    Group {
                        id: group_id.clone(),
                        codename,
                        is_folder,
                        docs,
                    },
                );
                work.push((group_id, sim));
            }
            inner.sink.clone()
        };
        sink.notify();
        for (group_id, sim) in work {
            Self::simulate(self.inner.clone(), group_id, sim, self.tick);
        }
        Ok(())
    }

    fn queue(&self) -> Vec<QueueItem> {
        self.lock().groups.iter().map(Group::to_item).collect()
    }

    fn answer(&self, question_id: &str, answer: ReviewAnswer) -> Result<(), FfiError> {
        let sink = {
            let mut inner = self.lock();
            let doc = inner
                .groups
                .iter_mut()
                .flat_map(|g| g.docs.iter_mut())
                .find(|d| d.question.as_ref().is_some_and(|q| q.id == question_id))
                .ok_or_else(|| FfiError::NotFound {
                    reason: format!("question {question_id}"),
                })?;
            if answer == ReviewAnswer::InkIt {
                doc.inked += 1;
            }
            doc.question = None;
            doc.state = DocState::Ready;
            doc.status_note = Some("ready".into());
            inner.sink.clone()
        };
        sink.notify();
        Ok(())
    }

    fn recent_reads(&self, n: u32) -> Vec<ReadEvent> {
        self.lock().reads.iter().take(n as usize).cloned().collect()
    }

    fn status(&self) -> Status {
        Status {
            mcp_listening: true,
            private_folder_display: self.private_folder_display.clone(),
        }
    }

    fn doctor(&self) -> Vec<String> {
        vec![
            "core: preview, no vault and no detection".into(),
            "mcp: no socket in a preview".into(),
            format!("private folder: {}", self.private_folder_display),
            crate::agents::AGENTS_NOT_CHECKED.into(),
        ]
    }
}

// ---------------------------------------------------------------------------
// Names
// ---------------------------------------------------------------------------

/// Codename proposal from a file name: extension dropped, lowercased, runs of
/// anything but ASCII letters and digits become one hyphen. The real core
/// uses kind, payer and year (brief); this is the preview's stand-in.
pub fn codename_from_name(name: &str) -> String {
    let stem = Utf8Path::new(name.trim()).file_stem().unwrap_or(name);
    slug(stem)
}

fn slug(s: &str) -> String {
    let mut out = String::new();
    let mut pending_hyphen = false;
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            if pending_hyphen && !out.is_empty() {
                out.push('-');
            }
            pending_hyphen = false;
            out.push(c.to_ascii_lowercase());
        } else {
            pending_hyphen = true;
        }
        if out.len() >= 60 {
            break;
        }
    }
    let out = out.trim_end_matches('-').to_string();
    if out.is_empty() {
        "document".into()
    } else {
        out
    }
}

/// Decision 5.7: `w2-acme-2025`, `w2-acme-2025-2`, ...
fn unique(base: &str, taken: &HashSet<String>) -> String {
    if !taken.contains(base) {
        return base.to_string();
    }
    (2..)
        .map(|n| format!("{base}-{n}"))
        .find(|c| !taken.contains(c))
        .unwrap_or_else(|| base.to_string())
}

fn extension(name: &str) -> String {
    Utf8Path::new(name)
        .extension()
        .unwrap_or("")
        .to_ascii_lowercase()
}

fn is_scan(name: &str) -> bool {
    matches!(
        extension(name).as_str(),
        "heic" | "heif" | "jpg" | "jpeg" | "png" | "tif" | "tiff"
    )
}

fn kind_label(name: &str) -> String {
    let n = name.to_ascii_lowercase();
    let label = if n.contains("w2") || n.contains("w-2") {
        "W-2"
    } else if n.contains("1099-int") || n.contains("1099int") {
        "1099-INT"
    } else if n.contains("1099") {
        "1099"
    } else if is_scan(name) {
        "scan"
    } else if n.contains("closing") {
        "closing statement"
    } else if n.contains("receipt") {
        "receipt"
    } else {
        "document"
    };
    label.into()
}

/// A stable, made-up count so the same name always inks the same amount.
fn inked_for(name: &str) -> u32 {
    let h = name
        .bytes()
        .fold(7u32, |h, b| h.wrapping_mul(31).wrapping_add(u32::from(b)));
    2 + h % 9
}

/// Files directly inside a dropped folder, sorted, hidden files skipped.
fn list_files(dir: &Utf8Path) -> Vec<String> {
    let mut names: Vec<String> = match dir.read_dir_utf8() {
        Ok(rd) => rd
            .filter_map(Result::ok)
            .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
            .map(|e| e.file_name().to_string())
            .filter(|n| !n.starts_with('.'))
            .collect(),
        Err(_) => Vec::new(),
    };
    names.sort();
    names
}

// ---------------------------------------------------------------------------
// Fixtures (invented names, no values)
// ---------------------------------------------------------------------------

fn doc(
    id: &str,
    codename: &str,
    original: &str,
    state: DocState,
    inked: u32,
    note: &str,
    loader: Loader,
) -> DocItem {
    DocItem {
        id: id.into(),
        codename: codename.into(),
        original_name: original.into(),
        // Fixtures live in an invented private folder; real drops set the real path.
        path: format!("~/Private/taxes-2025/{original}"),
        kind_label: kind_label(original),
        state,
        inked,
        status_note: Some(note.into()),
        loader,
        question: None,
    }
}

fn sample_question(doc_id: &str) -> ReviewQuestion {
    ReviewQuestion {
        id: format!("q-{doc_id}"),
        prompt: "Looks like an account number, but the scan is rough. Ink it?".into(),
        location: "page 3 of 11".into(),
        context_before: "…wire to".into(),
        value_length: 9,
    }
}

fn folder(docs: Vec<DocItem>) -> Group {
    Group {
        id: "g-1".into(),
        codename: "taxes-2025".into(),
        is_folder: true,
        docs,
    }
}

fn fixture(s: PreviewScenario) -> (Vec<Group>, Vec<ReadEvent>) {
    use DocState::*;
    let w2 = || {
        doc(
            "d-1",
            "w2-acme-2025",
            "W2_Acme_2025.pdf",
            Ready,
            6,
            "ready",
            Loader::Idle,
        )
    };
    let int = |state, note, loader| {
        doc(
            "d-2",
            "1099int-northwind-2025",
            "1099-INT_northwind.pdf",
            state,
            4,
            note,
            loader,
        )
    };
    let scan = |state, note, loader| {
        doc(
            "d-3",
            "receipt-4471",
            "IMG_4471.heic",
            state,
            2,
            note,
            loader,
        )
    };
    let closing = |state, inked, note, loader| {
        doc(
            "d-4",
            "closing-statement-2025",
            "closing-statement.pdf",
            state,
            inked,
            note,
            loader,
        )
    };
    match s {
        PreviewScenario::Empty => (Vec::new(), Vec::new()),
        PreviewScenario::Processing => (
            vec![folder(vec![
                w2(),
                int(Reading, "reading", Loader::Read),
                scan(Reading, "reading the scan", Loader::Ocr),
                closing(Queued, 0, "queued", Loader::Queued),
            ])],
            Vec::new(),
        ),
        PreviewScenario::Attention => {
            let mut held = closing(NeedsAttention, 10, "page 3 of 11", Loader::Idle);
            held.question = Some(sample_question(&held.id));
            (
                vec![folder(vec![
                    held,
                    w2(),
                    int(Ready, "ready", Loader::Idle),
                    scan(Ready, "ready", Loader::Idle),
                ])],
                Vec::new(),
            )
        }
        PreviewScenario::Ready => (
            vec![folder(vec![
                w2(),
                int(Ready, "ready", Loader::Idle),
                scan(Ready, "ready", Loader::Idle),
                closing(Ready, 11, "ready", Loader::Idle),
            ])],
            vec![ReadEvent {
                agent: "Claude Code".into(),
                codename: "w2-acme-2025".into(),
                at: SystemTime::now() - Duration::from_secs(120),
            }],
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unique_appends_the_first_free_suffix() {
        let taken: HashSet<String> = ["a".to_string(), "a-2".to_string()].into();
        assert_eq!(unique("a", &taken), "a-3");
        assert_eq!(unique("b", &taken), "b");
    }

    #[test]
    fn kind_labels() {
        assert_eq!(kind_label("W2_Acme.pdf"), "W-2");
        assert_eq!(kind_label("1099-INT_x.pdf"), "1099-INT");
        assert_eq!(kind_label("IMG_1.heic"), "scan");
        assert_eq!(kind_label("closing-statement.pdf"), "closing statement");
        assert_eq!(kind_label("notes.txt"), "document");
    }

    #[test]
    fn inked_is_stable_and_in_range() {
        assert_eq!(inked_for("a.pdf"), inked_for("a.pdf"));
        assert!((2..=10).contains(&inked_for("whatever.pdf")));
    }
}

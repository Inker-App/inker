//! `PreviewBackend`: fake data that drives the four popover states, so the app
//! runs end to end with no core wired.
//!
//! It never moves, copies, opens or reads the contents of a dropped file. It
//! only lists the entries of a dropped folder to make rows. All names and
//! counts in the fixed scenarios are invented.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, SystemTime};

use camino::{Utf8Path, Utf8PathBuf};
use inker_core::find::{normalize_query, text_snippet};

use crate::backend::{Backend, ChangeSink};
use crate::error::FfiError;
use crate::types::{
    Board, BoardEntry, DocDetails, DocItem, DocState, FindMatch, FolderItem, FolderSummary, Loader,
    MatchField, QueueItem, ReadEvent, ReviewAnswer, ReviewQuestion, Status, TextSpan,
};

/// The four popover boards, plus the ready board at scale.
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
    /// `AppFindScale`: 30 files across three years, two dropped folders.
    Scale,
}

#[derive(Debug, Clone)]
struct Group {
    id: String,
    codename: String,
    is_folder: bool,
    /// The folder's original name ("Taxes 2025"); the file name otherwise.
    name: String,
    path: String,
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

    fn to_entry(&self) -> BoardEntry {
        if self.is_folder {
            let mut children = self.docs.clone();
            children.sort_by_key(|d| std::cmp::Reverse(d.dropped_at));
            BoardEntry::Folder {
                item: FolderItem {
                    id: self.id.clone(),
                    codename: self.codename.clone(),
                    name: self.name.clone(),
                    path: self.path.clone(),
                    summary: FolderSummary::of(&children),
                    children,
                },
            }
        } else {
            BoardEntry::Doc {
                item: self.docs[0].clone(),
            }
        }
    }
}

#[derive(Debug)]
struct Inner {
    /// Newest first.
    groups: Vec<Group>,
    reads: Vec<ReadEvent>,
    /// One invented line of inked text per document id, for Find.
    texts: HashMap<String, String>,
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
                texts: HashMap::new(),
                sink: ChangeSink::noop(),
                next_id: 0,
            })),
            private_folder_display: private_folder_display.to_string(),
            tick,
        }
    }

    /// Frozen in one of the boards. Answers, new drops, Forget and Ink
    /// again still work.
    pub fn scenario(scenario: PreviewScenario) -> Self {
        Self::scenario_with_tick(scenario, SCENARIO_TICK)
    }

    /// A scenario whose simulated steps take `tick`, for tests.
    pub fn scenario_with_tick(scenario: PreviewScenario, tick: Duration) -> Self {
        let b = Self::live("~/Private", tick);
        {
            let mut inner = b.lock();
            let (groups, reads) = fixture(scenario);
            inner.texts = fixture_texts(&groups);
            inner.groups = groups;
            inner.reads = reads;
            inner.next_id = 100;
        }
        b
    }

    /// Every document, newest first.
    fn all_docs(inner: &Inner) -> Vec<DocItem> {
        let mut docs: Vec<DocItem> = inner.groups.iter().flat_map(|g| g.docs.clone()).collect();
        docs.sort_by_key(|d| std::cmp::Reverse(d.dropped_at));
        docs
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
        let mut planned: Vec<(String, bool, Vec<Utf8PathBuf>, Utf8PathBuf)> = Vec::new();
        for p in &paths {
            let name = p.file_name().unwrap_or("document").to_string();
            if p.is_dir() {
                let files = list_files(p).into_iter().map(|f| p.join(f)).collect();
                planned.push((name, true, files, p.clone()));
            } else {
                planned.push((name, false, vec![p.clone()], p.clone()));
            }
        }

        let mut work = Vec::new();
        let sink = {
            let mut inner = self.lock();
            for (name, is_folder, files, p) in planned {
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
                    d.year = year_in(file);
                    if is_folder {
                        d.folder_name = Some(name.clone());
                    }
                    docs.push(d);
                }
                let path = if is_folder {
                    p.to_string()
                } else {
                    files[0].to_string()
                };
                inner.groups.insert(
                    0,
                    Group {
                        id: group_id.clone(),
                        codename,
                        is_folder,
                        name: name.clone(),
                        path,
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
            clients: vec!["Claude".into()],
        }
    }

    fn board(&self) -> Board {
        let inner = self.lock();
        let docs = Self::all_docs(&inner);
        let entries = inner.groups.iter().map(Group::to_entry).collect();
        Board::assemble(
            entries,
            Board::count_tags(&docs),
            u32::try_from(docs.len()).unwrap_or(u32::MAX),
        )
    }

    fn find(&self, query: &str) -> Vec<FindMatch> {
        let q = normalize_query(query);
        if q.is_empty() {
            return Vec::new();
        }
        let ql = q.to_lowercase();
        let inner = self.lock();
        let mut out: Vec<FindMatch> = Vec::new();
        for d in Self::all_docs(&inner) {
            let fields: [(MatchField, Option<&str>); 7] = [
                (MatchField::FileName, Some(d.original_name.as_str())),
                (MatchField::Handle, Some(d.codename.as_str())),
                (MatchField::Kind, Some(d.kind_label.as_str())),
                (MatchField::Payer, d.payer.as_deref()),
                (MatchField::Year, None),
                (MatchField::Folder, d.folder_name.as_deref()),
                (MatchField::Tag, d.tag.as_deref()),
            ];
            let year = d.year.map(|y| y.to_string());
            let hit = fields.iter().find_map(|(field, hay)| {
                let hay = match field {
                    MatchField::Year => year.as_deref(),
                    _ => *hay,
                }?;
                let start = hay.to_lowercase().find(&ql)?;
                let matched = hay.get(start..start + ql.len())?.to_string();
                Some((*field, matched))
            });
            match hit {
                Some((field, matched)) => out.push(FindMatch {
                    exact_handle: field == MatchField::Handle && d.codename.len() == ql.len(),
                    doc: d,
                    field,
                    matched,
                    snippet: None,
                }),
                None => {
                    if let Some((snippet, matched)) =
                        inner.texts.get(&d.id).and_then(|t| text_snippet(t, &q))
                    {
                        out.push(FindMatch {
                            doc: d,
                            field: MatchField::Text,
                            matched,
                            snippet: Some(snippet),
                            exact_handle: false,
                        });
                    }
                }
            }
        }
        out.sort_by(|a, b| {
            a.field
                .cmp(&b.field)
                .then_with(|| b.doc.dropped_at.cmp(&a.doc.dropped_at))
        });
        out
    }

    fn locate(&self, codename: &str) -> Option<BoardEntry> {
        let inner = self.lock();
        for g in &inner.groups {
            if g.is_folder && g.codename == codename {
                return Some(g.to_entry());
            }
            if let Some(d) = g.docs.iter().find(|d| d.codename == codename) {
                return Some(BoardEntry::Doc { item: d.clone() });
            }
        }
        None
    }

    fn details(&self, id: &str) -> Result<DocDetails, FfiError> {
        let inner = self.lock();
        let (group, doc) = inner
            .groups
            .iter()
            .find_map(|g| g.docs.iter().find(|d| d.id == id).map(|d| (g, d)))
            .ok_or_else(|| FfiError::NotFound {
                reason: "no document with that id".into(),
            })?;
        let location = if group.is_folder {
            format!("{}/", group.name)
        } else {
            String::new()
        };
        Ok(DocDetails {
            location,
            dropped_at: doc.dropped_at,
            dropped_with: if group.is_folder {
                u32::try_from(group.docs.len().saturating_sub(1)).unwrap_or(u32::MAX)
            } else {
                0
            },
            last_read: inner
                .reads
                .iter()
                .find(|r| r.codename == doc.codename)
                .cloned(),
        })
    }

    fn forget(&self, id: &str) -> Result<(), FfiError> {
        let sink = {
            let mut inner = self.lock();
            let before =
                inner.groups.len() + inner.groups.iter().map(|g| g.docs.len()).sum::<usize>();
            for g in &mut inner.groups {
                g.docs.retain(|d| d.id != id);
            }
            inner.groups.retain(|g| g.id != id && !g.docs.is_empty());
            let after =
                inner.groups.len() + inner.groups.iter().map(|g| g.docs.len()).sum::<usize>();
            if before == after {
                return Err(FfiError::NotFound {
                    reason: "no document or folder with that id".into(),
                });
            }
            inner.sink.clone()
        };
        sink.notify();
        Ok(())
    }

    fn ink_again(&self, id: &str) -> Result<(), FfiError> {
        let (group_id, sim, sink) = {
            let mut inner = self.lock();
            let group = inner
                .groups
                .iter_mut()
                .find(|g| g.id == id || g.docs.iter().any(|d| d.id == id))
                .ok_or_else(|| FfiError::NotFound {
                    reason: "no document or folder with that id".into(),
                })?;
            let mut sim = Vec::new();
            for d in group
                .docs
                .iter_mut()
                .filter(|d| group.id == id || d.id == id)
            {
                d.state = DocState::Queued;
                d.status_note = Some("queued".into());
                d.loader = Loader::Queued;
                d.question = None;
                sim.push((
                    d.id.clone(),
                    is_scan(&d.original_name),
                    false,
                    d.inked.max(2),
                ));
            }
            (group.id.clone(), sim, inner.sink.clone())
        };
        sink.notify();
        Self::simulate(self.inner.clone(), group_id, sim, self.tick);
        Ok(())
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
        year: None,
        payer: None,
        tag: None,
        dropped_at: SystemTime::now(),
        folder_name: None,
    }
}

/// A four-digit year 2000-2099 standing alone in a file name.
fn year_in(name: &str) -> Option<u16> {
    let b = name.as_bytes();
    (0..b.len().saturating_sub(3)).find_map(|i| {
        let run = &b[i..i + 4];
        let standalone = i.checked_sub(1).is_none_or(|j| !b[j].is_ascii_digit())
            && b.get(i + 4).is_none_or(|c| !c.is_ascii_digit());
        if standalone && run.starts_with(b"20") && run.iter().all(u8::is_ascii_digit) {
            std::str::from_utf8(run).ok()?.parse().ok()
        } else {
            None
        }
    })
}

fn sample_question(doc_id: &str) -> ReviewQuestion {
    ReviewQuestion {
        id: format!("q-{doc_id}"),
        prompt: "Looks like an account number, but the scan is rough. Ink it?".into(),
        location: "page 3 of 11".into(),
        page_index: 2,
        context_before: "…wire to".into(),
        value_length: 9,
        span: Some(TextSpan {
            start: 120,
            end: 129,
        }),
    }
}

fn folder(docs: Vec<DocItem>) -> Group {
    let docs = docs
        .into_iter()
        .map(|mut d| {
            d.folder_name = Some("Taxes 2025".into());
            d.year = Some(2025);
            d
        })
        .collect();
    Group {
        id: "g-1".into(),
        codename: "taxes-2025".into(),
        is_folder: true,
        name: "Taxes 2025".into(),
        path: "~/Private/Taxes 2025".into(),
        docs,
    }
}

/// `AppFindScale`: 30 ready files. 2025 has a nine-file folder and five
/// loose files, 2024 a six-file folder and five loose, 2023 five loose.
/// Chips: Taxes 24, Banking 5, Legal 1. Invented names and payers.
fn scale() -> (Vec<Group>, Vec<ReadEvent>) {
    let base = SystemTime::now() - Duration::from_secs(300 * 86_400);
    let at = |days: u64| base + Duration::from_secs(days * 86_400);
    let mut next = 0u64;
    let mut id = || {
        next += 1;
        format!("s-{next}")
    };
    let ready = |id: &str,
                 code: &str,
                 name: &str,
                 label: &str,
                 inked: u32,
                 year: u16,
                 payer: Option<&str>,
                 tag: Option<&str>,
                 day: u64| {
        let mut d = doc(
            id,
            code,
            name,
            DocState::Ready,
            inked,
            "ready",
            Loader::Idle,
        );
        d.kind_label = label.into();
        d.year = Some(year);
        d.payer = payer.map(str::to_string);
        d.tag = tag.map(str::to_string);
        d.dropped_at = at(day);
        d
    };
    let single = |mut d: DocItem, gid: String, path_dir: &str| {
        d.path = format!("~/Private/{path_dir}{}", d.original_name);
        Group {
            id: gid,
            codename: d.codename.clone(),
            is_folder: false,
            name: d.original_name.clone(),
            path: d.path.clone(),
            docs: vec![d],
        }
    };
    let mut groups = Vec::new();

    // 2025: the folder (nine W-2s and 1099s from a payroll export), day 200.
    let folder_names = [
        (
            "w2-2025-3",
            "W2_2025_copy_b.pdf",
            "W-2",
            6,
            Some("Acme Robotics"),
        ),
        (
            "w2-2025-4",
            "W2_2025_copy_c.pdf",
            "W-2",
            6,
            Some("Acme Robotics"),
        ),
        (
            "1099-nec-2025",
            "1099-NEC_bluefin.pdf",
            "1099-NEC",
            4,
            Some("Bluefin Analytics"),
        ),
        (
            "1099-div-2025",
            "1099-DIV_northwind.pdf",
            "1099-DIV",
            4,
            Some("Northwind Funds"),
        ),
        (
            "1099-b-2025",
            "1099-B_northwind.pdf",
            "1099-B",
            5,
            Some("Northwind Funds"),
        ),
        ("1095-a-2025", "1095-A.pdf", "1095-A", 5, None),
        (
            "k1-1065-2025",
            "K1_tidewater.pdf",
            "K-1",
            4,
            Some("Tidewater Partners"),
        ),
        (
            "1098-2025-2",
            "1098_second_home.pdf",
            "1098",
            4,
            Some("Harbor Street CU"),
        ),
        (
            "1099-k-2025",
            "1099-K.pdf",
            "1099-K",
            3,
            Some("Cornerstone Pay"),
        ),
    ];
    let mut docs = Vec::new();
    for (code, name, label, inked, payer) in folder_names {
        let mut d = ready(
            &id(),
            code,
            name,
            label,
            inked,
            2025,
            payer,
            Some("Taxes"),
            200,
        );
        d.folder_name = Some("Taxes 2025".into());
        d.path = format!("~/Private/Taxes 2025/{name}");
        docs.push(d);
    }
    debug_assert_eq!(docs.iter().map(|d| d.inked).sum::<u32>(), 41);
    groups.push(Group {
        id: "f-2025".into(),
        codename: "folder-0412".into(),
        is_folder: true,
        name: "Taxes 2025".into(),
        path: "~/Private/Taxes 2025".into(),
        docs,
    });
    for (code, name, label, inked, payer, tag, day) in [
        (
            "w2-2025",
            "W2_AcmeRobotics_2025.pdf",
            "W-2",
            6,
            Some("Acme Robotics"),
            Some("Taxes"),
            230,
        ),
        (
            "w2-2025-2",
            "w2-bluefin.pdf",
            "W-2",
            6,
            Some("Bluefin Analytics"),
            Some("Taxes"),
            225,
        ),
        (
            "1099-int-2025",
            "IMG_4471.heic",
            "1099-INT",
            4,
            Some("Harbor Street CU"),
            Some("Taxes"),
            220,
        ),
        (
            "1098-2025",
            "1098_harbor_2025.pdf",
            "1098",
            4,
            Some("Harbor Street CU"),
            Some("Taxes"),
            215,
        ),
        (
            "bank-statement-2025",
            "statement-2025-03.pdf",
            "bank statement",
            7,
            None,
            Some("Banking"),
            210,
        ),
    ] {
        let d = ready(&id(), code, name, label, inked, 2025, payer, tag, day);
        groups.push(single(d, id(), ""));
    }

    // 2024: a six-file folder, day 60, and five loose files.
    let mut docs = Vec::new();
    for (code, name, label, inked, payer) in [
        (
            "w2-2024-2",
            "W2_2024_copy_b.pdf",
            "W-2",
            6,
            Some("Acme Robotics"),
        ),
        (
            "w2-2024-3",
            "W2_2024_copy_c.pdf",
            "W-2",
            6,
            Some("Acme Robotics"),
        ),
        (
            "1099-nec-2024",
            "1099-NEC_bluefin_2024.pdf",
            "1099-NEC",
            4,
            Some("Bluefin Analytics"),
        ),
        (
            "1099-int-2024",
            "1099-INT_harbor_2024.pdf",
            "1099-INT",
            4,
            Some("Harbor Street CU"),
        ),
        ("1095-b-2024", "1095-B.pdf", "1095-B", 5, None),
        (
            "k1-1065-2024",
            "K1_tidewater_2024.pdf",
            "K-1",
            4,
            Some("Tidewater Partners"),
        ),
    ] {
        let mut d = ready(
            &id(),
            code,
            name,
            label,
            inked,
            2024,
            payer,
            Some("Taxes"),
            60,
        );
        d.folder_name = Some("Taxes 2024".into());
        d.path = format!("~/Private/Taxes 2024/{name}");
        docs.push(d);
    }
    groups.push(Group {
        id: "f-2024".into(),
        codename: "folder-0377".into(),
        is_folder: true,
        name: "Taxes 2024".into(),
        path: "~/Private/Taxes 2024".into(),
        docs,
    });
    for (code, name, label, inked, payer, tag, day) in [
        (
            "w2-2024",
            "w2-acme-2024.pdf",
            "W-2",
            6,
            Some("Acme Robotics"),
            Some("Taxes"),
            90,
        ),
        (
            "1098-2024",
            "1098_harbor_2024.pdf",
            "1098",
            4,
            Some("Harbor Street CU"),
            Some("Taxes"),
            85,
        ),
        (
            "1099-div-2024",
            "1099-DIV_2024.pdf",
            "1099-DIV",
            4,
            Some("Northwind Funds"),
            Some("Taxes"),
            80,
        ),
        (
            "bank-statement-2024",
            "statement-2024-12.pdf",
            "bank statement",
            7,
            None,
            Some("Banking"),
            75,
        ),
        (
            "bank-statement-2024-2",
            "statement-2024-06.pdf",
            "bank statement",
            7,
            None,
            Some("Banking"),
            70,
        ),
    ] {
        let d = ready(&id(), code, name, label, inked, 2024, payer, tag, day);
        groups.push(single(d, id(), ""));
    }

    // 2023: five loose files.
    for (code, name, label, inked, payer, tag, day) in [
        (
            "1040-2023",
            "1040_2023.pdf",
            "1040",
            12,
            None,
            Some("Taxes"),
            20,
        ),
        (
            "k1-1065-2023",
            "K1_tidewater_2023.pdf",
            "K-1",
            4,
            Some("Tidewater Partners"),
            Some("Taxes"),
            15,
        ),
        (
            "bank-statement-2023",
            "statement-2023-12.pdf",
            "bank statement",
            7,
            None,
            Some("Banking"),
            10,
        ),
        (
            "bank-statement-2023-2",
            "statement-2023-06.pdf",
            "bank statement",
            7,
            None,
            Some("Banking"),
            5,
        ),
        (
            "closing-statement-2023",
            "closing-disclosure.pdf",
            "closing statement",
            11,
            None,
            Some("Legal"),
            1,
        ),
    ] {
        let d = ready(&id(), code, name, label, inked, 2023, payer, tag, day);
        groups.push(single(d, id(), ""));
    }

    groups.sort_by_key(|g| std::cmp::Reverse(g.docs.iter().map(|d| d.dropped_at).max()));
    let reads = vec![ReadEvent {
        agent: "Claude".into(),
        codename: "w2-2025-2".into(),
        at: SystemTime::now() - Duration::from_secs(120),
    }];
    (groups, reads)
}

/// One invented line of inked text per fixture document, so Find has
/// something to match. Tokens only, never a value.
fn fixture_texts(groups: &[Group]) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for d in groups.iter().flat_map(|g| g.docs.iter()) {
        let line = match d.kind_label.as_str() {
            "1098" => "Mortgage Interest Statement for [PERSON_1] from [ORG_1]",
            "W-2" => "Wage and Tax Statement · Employee [PERSON_1] · Employer [ORG_1]",
            "1099-INT" => "Interest Income · Payer [ORG_1] · Recipient [PERSON_1]",
            "bank statement" => "Statement period · Account [ACCOUNT_1] · Ending balance",
            "closing statement" => "Closing Disclosure · Borrower [PERSON_1] · Lender [ORG_1]",
            _ => "Form for [PERSON_1]",
        };
        out.insert(d.id.clone(), line.to_string());
    }
    out
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
            "1099-int-northwind-2025",
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
        PreviewScenario::Scale => scale(),
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
                agent: "Claude".into(),
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

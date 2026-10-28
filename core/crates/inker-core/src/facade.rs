//! The `Core` facade: one struct owning the vault, the index and the host
//! paths, with the operations the MCP server and the Swift shell call.
//!
//! The pipeline steps that need extraction and detection live in other
//! crates. The orchestrator calls [`Core::begin_ingest`] (move files, create
//! `Queued` records), moves records through `Reading` with
//! [`Core::set_state`], then hands the results to
//! [`Core::complete_processing`].

use std::fs;

use camino::{Utf8Path, Utf8PathBuf};
use chrono::Utc;

use crate::codename::{self, CodenamePolicy};
use crate::index::{DerivedData, Index};
use crate::ports::{HostPaths, KeyStore};
use crate::tokenize::{self, Tokenized};
use crate::vault::{Vault, find_tokens};
use crate::{
    AgentRead, Description, Detection, DocKind, DocState, DocumentId, DocumentRecord, Error,
    Extracted, FolderId, FolderRecord, FolderSummary, Handle, IngestMode, ListEntry, QuestionId,
    Resolved, ResolvedDocument, ResolvedFolder, ResolvedPage, Result, ReviewAnswer, ReviewQuestion,
    TokenKind, Treatment, WriteOutcome, ingest, rehydrate,
};

/// Kind given to files written by agents through `write`.
pub const OUTPUT_KIND: &str = "output";
/// Kind shown for folders in listings and `describe`.
pub const FOLDER_KIND: &str = "folder";

/// Roles whose organisation names a document (for the codename).
const NAMING_ROLES: &[&str] = &[
    "employer", "payer", "lender", "issuer", "filer", "trustee", "bank",
];

/// The platform-independent core. Thread-safe: share it behind an `Arc`.
#[derive(Debug)]
pub struct Core {
    paths: HostPaths,
    vault: Vault,
    index: Index,
    policy: CodenamePolicy,
}

enum Target {
    Doc(DocumentRecord),
    Folder(FolderRecord),
}

fn discriminator(id: DocumentId) -> u32 {
    let b = id.0.as_bytes();
    u32::from_le_bytes([b[0], b[1], b[2], b[3]])
}

fn gate(rec: &DocumentRecord) -> Result<()> {
    let handle = Handle::new(rec.codename.clone()).to_string();
    match rec.state {
        DocState::Ready => Ok(()),
        DocState::NeedsAttention => Err(Error::PendingReview(handle)),
        DocState::Failed => Err(Error::NotReady(format!("{handle} could not be processed"))),
        DocState::Queued | DocState::Reading | DocState::Inking => Err(Error::NotReady(handle)),
    }
}

fn folder_state(s: &FolderSummary) -> DocState {
    if s.ready > 0 {
        DocState::Ready
    } else if s.inking > 0 {
        DocState::Inking
    } else if s.needs_attention > 0 {
        DocState::NeedsAttention
    } else if s.failed > 0 {
        DocState::Failed
    } else {
        DocState::Queued
    }
}

fn doc_entry(r: &DocumentRecord) -> ListEntry {
    ListEntry {
        handle: Handle::new(r.codename.clone()),
        kind: r.kind.clone(),
        year: r.year,
        state: r.state,
        inked: r.inked,
        is_folder: false,
    }
}

/// The text a detection covers, if it fits.
fn detected_text<'a>(ex: &'a Extracted, d: &Detection) -> Option<&'a str> {
    let text = match &d.field {
        Some(name) => ex
            .fields
            .iter()
            .find(|f| &f.name == name)
            .map(|f| f.value.as_str())?,
        None => ex
            .pages
            .iter()
            .find(|p| p.page == d.page)
            .map(|p| p.text.as_str())?,
    };
    text.get(d.span.start..d.span.end)
}

/// The organisation that names the document: an Org detection with a naming
/// role first, then the most precise Org detection.
fn naming_org(ex: &Extracted, dets: &[Detection]) -> Option<String> {
    let mut orgs: Vec<&Detection> = dets.iter().filter(|d| d.kind == TokenKind::Org).collect();
    orgs.sort_by_key(|d| {
        let named = d.role.as_deref().is_some_and(|r| NAMING_ROLES.contains(&r));
        (!named, tokenize::source_rank(&d.source))
    });
    orgs.into_iter()
        .find_map(|d| detected_text(ex, d).map(str::to_string))
}

impl Core {
    /// Open the core: create the private folder and data dir if needed,
    /// unlock the vault at `<data_dir>/vault.bin` and open
    /// `<data_dir>/index.db`.
    pub fn open(paths: HostPaths, keystore: &dyn KeyStore) -> Result<Self> {
        fs::create_dir_all(&paths.private_folder)?;
        fs::create_dir_all(&paths.data_dir)?;
        let canon = |p: &Utf8Path| -> Result<Utf8PathBuf> {
            Utf8PathBuf::from_path_buf(fs::canonicalize(p)?)
                .map_err(|_| Error::Ingest("path is not valid UTF-8".into()))
        };
        let paths = HostPaths {
            private_folder: canon(&paths.private_folder)?,
            data_dir: canon(&paths.data_dir)?,
        };
        let vault = Vault::open(&paths.data_dir.join("vault.bin"), keystore)?;
        let index = Index::open(&paths.data_dir.join("index.db"))?;
        Ok(Self {
            paths,
            vault,
            index,
            policy: CodenamePolicy::default(),
        })
    }

    /// Choose what goes into new codenames (decision pending with Ben).
    pub fn with_codename_policy(mut self, policy: CodenamePolicy) -> Self {
        self.policy = policy;
        self
    }

    /// The host paths, canonicalised.
    pub fn paths(&self) -> &HostPaths {
        &self.paths
    }

    fn lookup(&self, handle: &Handle) -> Result<Target> {
        if let Some(d) = self.index.find_by_codename(&handle.codename)? {
            return Ok(Target::Doc(d));
        }
        if let Some(f) = self.index.find_folder_by_codename(&handle.codename)? {
            return Ok(Target::Folder(f));
        }
        Err(Error::UnknownHandle(handle.to_string()))
    }

    fn folder_entry(&self, f: &FolderRecord) -> Result<ListEntry> {
        let s = self.index.folder_summary(f.id)?;
        Ok(ListEntry {
            handle: Handle::new(f.codename.clone()),
            kind: DocKind(FOLDER_KIND.into()),
            year: None,
            state: folder_state(&s),
            inked: s.inked,
            is_folder: true,
        })
    }

    /// `list`: every folder and every document not inside a folder.
    pub fn list(&self) -> Result<Vec<ListEntry>> {
        let mut out = Vec::new();
        for f in self.index.list_folders()? {
            out.push(self.folder_entry(&f)?);
        }
        for d in self.index.list_documents()? {
            if d.folder.is_none() {
                out.push(doc_entry(&d));
            }
        }
        Ok(out)
    }

    /// `resolve`: tokenized text for a ready document, or a folder listing.
    /// Held documents return [`Error::PendingReview`], in-flight and failed
    /// ones [`Error::NotReady`].
    pub fn resolve(&self, handle: &Handle) -> Result<Resolved> {
        match self.lookup(handle)? {
            Target::Doc(rec) => {
                gate(&rec)?;
                let pages = self.index.pages(rec.id)?;
                let fields = self
                    .index
                    .derived(rec.id)?
                    .map(|d| d.fields)
                    .unwrap_or_default();
                Ok(Resolved::Document(ResolvedDocument {
                    handle: Handle::new(rec.codename),
                    kind: rec.kind,
                    year: rec.year,
                    pages,
                    fields,
                }))
            }
            Target::Folder(f) => {
                let children = self
                    .index
                    .folder_children(f.id)?
                    .iter()
                    .map(doc_entry)
                    .collect();
                Ok(Resolved::Folder(ResolvedFolder {
                    handle: Handle::new(f.codename),
                    children,
                }))
            }
        }
    }

    /// `describe`: the role map and treatment counts. Roles are only given
    /// for ready documents.
    pub fn describe(&self, handle: &Handle) -> Result<Description> {
        match self.lookup(handle)? {
            Target::Doc(rec) => {
                let derived = if rec.state == DocState::Ready {
                    self.index.derived(rec.id)?
                } else {
                    None
                };
                let derived = derived.unwrap_or_default();
                Ok(Description {
                    handle: Handle::new(rec.codename),
                    kind: rec.kind,
                    year: rec.year,
                    state: rec.state,
                    roles: derived.roles,
                    treatments: derived.treatments,
                })
            }
            Target::Folder(f) => {
                let entry = self.folder_entry(&f)?;
                Ok(Description {
                    handle: entry.handle,
                    kind: entry.kind,
                    year: None,
                    state: entry.state,
                    roles: Vec::new(),
                    treatments: Vec::new(),
                })
            }
        }
    }

    /// The record behind a document handle.
    pub fn find_document(&self, handle: &Handle) -> Result<DocumentRecord> {
        self.index
            .find_by_codename(&handle.codename)?
            .ok_or_else(|| Error::UnknownHandle(handle.to_string()))
    }

    /// A record by id.
    pub fn document(&self, id: DocumentId) -> Result<DocumentRecord> {
        self.index
            .document(id)?
            .ok_or_else(|| Error::UnknownHandle(id.0.to_string()))
    }

    pub fn folder_summary(&self, id: FolderId) -> Result<FolderSummary> {
        self.index.folder_summary(id)
    }

    /// `write`: rehydrate the agent's tokenized `content` and save it as
    /// `name` beside the source document (or inside the source folder), only
    /// ever under the private folder. Returns the path and a new handle whose
    /// `resolve` gives back the tokenized view. Re-writing the same name
    /// replaces an earlier output; any other existing file is refused.
    pub fn write(&self, handle: &Handle, name: &str, content: &str) -> Result<WriteOutcome> {
        let (dir, folder, year, source_code, source_roles) = match self.lookup(handle)? {
            Target::Doc(rec) => {
                gate(&rec)?;
                let dir = rec
                    .path
                    .parent()
                    .map(Utf8Path::to_owned)
                    .ok_or_else(|| Error::Refused("no parent".into()))?;
                let roles = self
                    .index
                    .derived(rec.id)?
                    .map(|d| d.roles)
                    .unwrap_or_default();
                (dir, rec.folder, rec.year, rec.codename, roles)
            }
            Target::Folder(f) => (f.path, Some(f.id), None, f.codename, Vec::new()),
        };
        let target = rehydrate::resolve_target(&self.paths.private_folder, &dir, name)?;
        let existing_output = match self.index.find_by_path(&target)? {
            Some(rec) if self.index.output_source(rec.id)?.is_some() => Some(rec),
            _ => None,
        };
        let (path, proposed) = rehydrate::write(
            &self.vault,
            &self.paths.private_folder,
            &dir,
            name,
            content,
            &source_code,
            existing_output.is_some(),
        )?;

        let present: Vec<(String, u32)> = find_tokens(content)
            .into_iter()
            .map(|(_, l, i)| (l, i))
            .collect();
        let inked = u32::try_from(present.len()).unwrap_or(u32::MAX);
        let roles = source_roles
            .into_iter()
            .filter(|r| {
                present
                    .iter()
                    .any(|(l, i)| *l == r.token.kind.label() && *i == r.token.index)
            })
            .collect();
        let pages = [ResolvedPage {
            page: 1,
            text: content.to_string(),
        }];

        let rec = match existing_output {
            Some(mut rec) => {
                rec.inked = inked;
                self.index.update_document(&rec)?;
                rec
            }
            None => {
                let codename = codename::disambiguate(proposed, |c| {
                    self.index.codename_taken(c).unwrap_or(true)
                });
                let now = Utc::now();
                let rec = DocumentRecord {
                    id: DocumentId::new(),
                    codename,
                    path: path.clone(),
                    original_name: name.to_string(),
                    kind: DocKind(OUTPUT_KIND.into()),
                    year,
                    state: DocState::Ready,
                    inked,
                    folder,
                    ingest: IngestMode::Copy,
                    created_at: now,
                    updated_at: now,
                    status_note: None,
                };
                self.index.insert_document(&rec, false)?;
                self.index.record_output(rec.id, &source_code)?;
                rec
            }
        };
        self.index.store_pages(rec.id, &pages)?;
        self.index.store_derived(
            rec.id,
            &DerivedData {
                fields: Vec::new(),
                roles,
                treatments: Vec::new(),
            },
        )?;
        tracing::info!(source = %source_code, output = %rec.codename, "write");
        Ok(WriteOutcome {
            path,
            handle: Handle::new(rec.codename),
        })
    }

    /// Unanswered attention-card questions across all documents.
    pub fn pending_reviews(&self) -> Result<Vec<ReviewQuestion>> {
        self.index.questions(None, false)
    }

    /// Record the user's answer. When the last open question of a document is
    /// answered, the document is re-inked with the answers applied and
    /// becomes `Ready`. Returns the document's state afterwards.
    pub fn answer_review(&self, question: QuestionId, answer: ReviewAnswer) -> Result<DocState> {
        let q = self.index.answer_question(question, answer)?;
        let rec = self.document(q.document)?;
        if !self.index.questions(Some(rec.id), false)?.is_empty()
            || rec.state != DocState::NeedsAttention
        {
            return Ok(rec.state);
        }
        let sealed = self.index.sealed_source(rec.id)?.ok_or_else(|| {
            Error::Index(format!("{}: text for re-inking is missing", rec.codename))
        })?;
        let extracted: Extracted = serde_json::from_slice(&self.vault.unseal(&sealed)?)?;
        let mut dets = self.index.detections(rec.id)?;
        for (i, a) in self.index.answered_detections(rec.id)? {
            if let Some(d) = dets.get_mut(i) {
                d.treatment = match a {
                    ReviewAnswer::InkIt => Treatment::Tokenize,
                    ReviewAnswer::LeaveIt => Treatment::Keep,
                };
                d.needs_review = false;
            }
        }
        self.index.set_state(rec.id, DocState::Inking, None)?;
        match self.ink(&rec, &extracted, &dets) {
            Ok(_) => {}
            Err(e) => {
                self.index
                    .set_state(rec.id, DocState::Failed, Some("could not ink this file"))?;
                return Err(e);
            }
        }
        self.index.delete_sealed_source(rec.id)?;
        self.index.set_state(rec.id, DocState::Ready, None)?;
        tracing::info!(codename = %rec.codename, "review answered; ready");
        Ok(DocState::Ready)
    }

    /// Audit log: `agent` read `handle`.
    pub fn record_agent_read(&self, agent: &str, handle: &Handle) -> Result<AgentRead> {
        self.lookup(handle)?;
        self.index.record_agent_read(agent, &handle.codename)
    }

    /// The `n` most recent agent reads, newest first.
    pub fn recent_reads(&self, n: usize) -> Result<Vec<AgentRead>> {
        self.index.recent_reads(n)
    }

    /// Move or copy each path into the private folder and create a `Queued`
    /// record for every new file (folders contribute each file inside).
    /// Already-indexed files are skipped. Records start with a provisional
    /// `doc-NNNN` codename, so file names never reach a handle.
    pub fn begin_ingest(&self, paths: &[Utf8PathBuf], mode: IngestMode) -> Result<Vec<DocumentId>> {
        let mut ids = Vec::new();
        for p in paths {
            let outcome = ingest::ingest(&self.paths.private_folder, p, mode)?;
            if outcome.is_dir {
                let folder = self.folder_for_dir(&outcome.path)?;
                for file in ingest::list_files(&outcome.path)? {
                    if let Some(id) = self.new_record(&file, Some(folder.id), mode)? {
                        ids.push(id);
                    }
                }
            } else {
                let folder = if outcome.in_place {
                    self.containing_folder(&outcome.path)?
                } else {
                    None
                };
                if let Some(id) = self.new_record(&outcome.path, folder, mode)? {
                    ids.push(id);
                }
            }
        }
        Ok(ids)
    }

    fn folder_for_dir(&self, dir: &Utf8Path) -> Result<FolderRecord> {
        if let Some(f) = self.index.find_folder_by_path(dir)? {
            return Ok(f);
        }
        let id = crate::FolderId::new();
        let b = id.0.as_bytes();
        let base = codename::propose_folder(
            dir.file_name().unwrap_or(""),
            u32::from_le_bytes([b[0], b[1], b[2], b[3]]),
        );
        let f = FolderRecord {
            id,
            codename: codename::disambiguate(base, |c| {
                self.index.codename_taken(c).unwrap_or(true)
            }),
            path: dir.to_owned(),
            created_at: Utc::now(),
        };
        self.index.insert_folder(&f)?;
        Ok(f)
    }

    fn containing_folder(&self, file: &Utf8Path) -> Result<Option<FolderId>> {
        Ok(self
            .index
            .list_folders()?
            .into_iter()
            .filter(|f| file.starts_with(&f.path))
            .max_by_key(|f| f.path.as_str().len())
            .map(|f| f.id))
    }

    fn new_record(
        &self,
        path: &Utf8Path,
        folder: Option<FolderId>,
        mode: IngestMode,
    ) -> Result<Option<DocumentId>> {
        if self.index.find_by_path(path)?.is_some() {
            return Ok(None);
        }
        let id = DocumentId::new();
        let base = codename::propose(
            CodenamePolicy::TypeYear,
            &DocKind::unknown(),
            None,
            None,
            discriminator(id),
        );
        let now = Utc::now();
        let rec = DocumentRecord {
            id,
            codename: codename::disambiguate(base, |c| {
                self.index.codename_taken(c).unwrap_or(true)
            }),
            path: path.to_owned(),
            original_name: path.file_name().unwrap_or_default().to_string(),
            kind: DocKind::unknown(),
            year: None,
            state: DocState::Queued,
            inked: 0,
            folder,
            ingest: mode,
            created_at: now,
            updated_at: now,
            status_note: None,
        };
        self.index.insert_document(&rec, true)?;
        Ok(Some(id))
    }

    /// Move a document through the state machine (e.g. `Reading` with a
    /// status note while extraction runs, `Failed` when it cannot).
    pub fn set_state(
        &self,
        id: DocumentId,
        state: DocState,
        note: Option<&str>,
    ) -> Result<DocumentRecord> {
        self.index.set_state(id, state, note)
    }

    /// Tokenize and store the results of extraction and detection, give the
    /// document its final codename, and set it `Ready`, or `NeedsAttention`
    /// with one review question per detection that `needs_review`. A bad
    /// detection marks the document `Failed` and returns the error.
    pub fn complete_processing(
        &self,
        id: DocumentId,
        extracted: &Extracted,
        detections: &[Detection],
        kind: DocKind,
        year: Option<u16>,
    ) -> Result<DocState> {
        let mut rec = self.document(id)?;
        if rec.state == DocState::Queued {
            rec = self.index.set_state(id, DocState::Reading, None)?;
        }
        rec = self
            .index
            .set_state(id, DocState::Inking, rec.status_note.as_deref())?;

        if self.index.is_provisional(id)? {
            let org = naming_org(extracted, detections);
            let base =
                codename::propose(self.policy, &kind, org.as_deref(), year, discriminator(id));
            let code = codename::disambiguate(base, |c| {
                c != rec.codename.as_str() && self.index.codename_taken(c).unwrap_or(true)
            });
            self.index.set_codename(id, &code)?;
            rec.codename = code;
        }
        rec.kind = kind;
        rec.year = year;
        self.index.update_document(&rec)?;

        if let Err(e) = self.ink(&rec, extracted, detections) {
            self.index
                .set_state(id, DocState::Failed, Some("could not ink this file"))?;
            return Err(e);
        }

        self.index.clear_questions(id)?;
        let review: Vec<(usize, &Detection)> = detections
            .iter()
            .enumerate()
            .filter(|(_, d)| d.needs_review)
            .collect();
        if review.is_empty() {
            self.index.delete_sealed_source(id)?;
            self.index.set_state(id, DocState::Ready, None)?;
            tracing::info!(codename = %rec.codename, "ready");
            return Ok(DocState::Ready);
        }
        let sealed = self.vault.seal(&serde_json::to_vec(extracted)?)?;
        self.index.store_sealed_source(id, &sealed)?;
        for (i, d) in &review {
            self.index
                .add_question(id, *i, d.page, &d.kind, d.review_prompt.as_deref())?;
        }
        self.index.set_state(id, DocState::NeedsAttention, None)?;
        tracing::info!(codename = %rec.codename, questions = review.len(), "needs attention");
        Ok(DocState::NeedsAttention)
    }

    /// Tokenize and store pages, derived data, detections and the inked count.
    fn ink(
        &self,
        rec: &DocumentRecord,
        extracted: &Extracted,
        dets: &[Detection],
    ) -> Result<Tokenized> {
        let t = tokenize::tokenize(&self.vault, extracted, dets)?;
        self.index.store_pages(rec.id, &t.pages)?;
        self.index.store_derived(
            rec.id,
            &DerivedData {
                fields: t.fields.clone(),
                roles: t.roles.clone(),
                treatments: t.treatments.clone(),
            },
        )?;
        self.index.store_detections(rec.id, dets)?;
        let mut current = self.document(rec.id)?;
        current.inked = t.inked;
        self.index.update_document(&current)?;
        Ok(t)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault::MemoryKeyStore;
    use crate::{Codename, DetectionSource, PageText, Span, TextOrigin};
    use std::fs;

    struct Fx {
        _t: tempfile::TempDir,
        root: Utf8PathBuf,
        core: Core,
    }

    fn fx() -> Fx {
        let t = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(t.path().canonicalize().unwrap()).unwrap();
        fs::create_dir_all(root.join("Downloads")).unwrap();
        let paths = HostPaths {
            private_folder: root.join("Private"),
            data_dir: root.join("data"),
        };
        let core = Core::open(paths, &MemoryKeyStore::new().unwrap()).unwrap();
        Fx { _t: t, root, core }
    }

    fn drop_file(f: &Fx, name: &str, text: &str) -> Utf8PathBuf {
        let p = f.root.join("Downloads").join(name);
        fs::write(&p, text).unwrap();
        p
    }

    fn ex(text: &str) -> Extracted {
        Extracted {
            pages: vec![PageText {
                page: 1,
                text: text.into(),
                origin: TextOrigin::Ocr,
            }],
            fields: vec![],
        }
    }

    fn d(text: &str, needle: &str, kind: TokenKind, review: bool) -> Detection {
        let start = text.find(needle).unwrap();
        Detection {
            page: 1,
            span: Span {
                start,
                end: start + needle.len(),
            },
            kind,
            source: DetectionSource::Model { name: "m".into() },
            confidence: if review { 0.4 } else { 0.95 },
            treatment: Treatment::Tokenize,
            needs_review: review,
            review_prompt: review
                .then(|| "Looks like an account number, but the scan is rough.".into()),
            role: None,
            generalized: None,
            field: None,
        }
    }

    fn h(code: &str) -> Handle {
        Handle::new(Codename::parse(code).unwrap())
    }

    #[test]
    fn paths_and_policy() {
        let f = fx();
        assert!(f.core.paths().private_folder.is_dir());
        assert!(f.core.paths().data_dir.join("index.db").exists());
        let core = f.core.with_codename_policy(CodenamePolicy::TypeYear);
        let text = "Payer Placeholder Bank";
        let ids = core
            .begin_ingest(&[drop_file_root(&f.root, "a.txt", text)], IngestMode::Move)
            .unwrap();
        core.complete_processing(
            ids[0],
            &ex(text),
            &[d(text, "Placeholder Bank", TokenKind::Org, false)],
            DocKind("1099-int".into()),
            Some(2025),
        )
        .unwrap();
        assert_eq!(
            core.document(ids[0]).unwrap().codename.as_str(),
            "1099int-2025"
        );
    }

    fn drop_file_root(root: &Utf8PathBuf, name: &str, text: &str) -> Utf8PathBuf {
        let p = root.join("Downloads").join(name);
        fs::write(&p, text).unwrap();
        p
    }

    #[test]
    fn provisional_codename_does_not_leak_file_name() {
        let f = fx();
        let ids = f
            .core
            .begin_ingest(
                &[drop_file(&f, "Testy McFakename W2.txt", "x")],
                IngestMode::Move,
            )
            .unwrap();
        let rec = f.core.document(ids[0]).unwrap();
        assert!(
            rec.codename.as_str().starts_with("doc-"),
            "{}",
            rec.codename
        );
        assert!(!rec.codename.as_str().contains("mcfakename"));
    }

    #[test]
    fn needs_attention_then_ink_it() {
        let f = fx();
        let text = "Acct 98765432 ref 000-12-3456";
        let ids = f
            .core
            .begin_ingest(&[drop_file(&f, "s.txt", text)], IngestMode::Move)
            .unwrap();
        let dets = [
            d(text, "98765432", TokenKind::Account, true),
            d(text, "000-12-3456", TokenKind::Ssn, false),
        ];
        let st = f
            .core
            .complete_processing(
                ids[0],
                &ex(text),
                &dets,
                DocKind("statement".into()),
                Some(2025),
            )
            .unwrap();
        assert_eq!(st, DocState::NeedsAttention);
        let code = f.core.document(ids[0]).unwrap().codename;
        let handle = Handle::new(code);
        assert!(matches!(
            f.core.resolve(&handle),
            Err(Error::PendingReview(_))
        ));
        assert!(matches!(
            f.core.write(&handle, "x.md", "x"),
            Err(Error::PendingReview(_))
        ));
        let desc = f.core.describe(&handle).unwrap();
        assert_eq!(desc.state, DocState::NeedsAttention);
        assert!(desc.roles.is_empty());

        let qs = f.core.pending_reviews().unwrap();
        assert_eq!(qs.len(), 1);
        assert_eq!(qs[0].kind, TokenKind::Account);
        assert_eq!(
            f.core.answer_review(qs[0].id, ReviewAnswer::InkIt).unwrap(),
            DocState::Ready
        );
        assert!(f.core.pending_reviews().unwrap().is_empty());
        let Resolved::Document(doc) = f.core.resolve(&handle).unwrap() else {
            panic!()
        };
        assert_eq!(doc.pages[0].text, "Acct [ACCOUNT_1] ref [SSN_1]");
        assert!(matches!(
            f.core.answer_review(qs[0].id, ReviewAnswer::LeaveIt),
            Err(Error::Index(_))
        ));
    }

    #[test]
    fn leave_it_keeps_value_for_this_document() {
        let f = fx();
        let text = "Order 4471 shipped";
        let ids = f
            .core
            .begin_ingest(&[drop_file(&f, "r.txt", text)], IngestMode::Move)
            .unwrap();
        let dets = [
            d(text, "4471", TokenKind::Account, true),
            d(text, "shipped", TokenKind::Custom("x".into()), true),
        ];
        f.core
            .complete_processing(ids[0], &ex(text), &dets, DocKind("receipt".into()), None)
            .unwrap();
        let qs = f.core.pending_reviews().unwrap();
        assert_eq!(qs.len(), 2);
        // One answer is not enough.
        assert_eq!(
            f.core
                .answer_review(qs[0].id, ReviewAnswer::LeaveIt)
                .unwrap(),
            DocState::NeedsAttention
        );
        assert_eq!(
            f.core.answer_review(qs[1].id, ReviewAnswer::InkIt).unwrap(),
            DocState::Ready
        );
        let rec = f.core.document(ids[0]).unwrap();
        assert!(rec.codename.as_str().starts_with("receipt-"));
        let Resolved::Document(doc) = f.core.resolve(&Handle::new(rec.codename)).unwrap() else {
            panic!()
        };
        assert_eq!(doc.pages[0].text, "Order 4471 [X_1]");
        assert_eq!(rec.inked, 1);
    }

    #[test]
    fn bad_detections_fail_the_document() {
        let f = fx();
        let ids = f
            .core
            .begin_ingest(&[drop_file(&f, "a.txt", "abc")], IngestMode::Move)
            .unwrap();
        let mut bad = d("abc", "abc", TokenKind::Ssn, false);
        bad.span = Span { start: 0, end: 50 };
        assert!(
            f.core
                .complete_processing(ids[0], &ex("abc"), &[bad], DocKind::unknown(), None)
                .is_err()
        );
        let rec = f.core.document(ids[0]).unwrap();
        assert_eq!(rec.state, DocState::Failed);
        assert!(matches!(
            f.core.resolve(&Handle::new(rec.codename)),
            Err(Error::NotReady(_))
        ));
    }

    #[test]
    fn set_state_and_reprocess_rules() {
        let f = fx();
        let ids = f
            .core
            .begin_ingest(&[drop_file(&f, "a.txt", "abc")], IngestMode::Move)
            .unwrap();
        let r = f
            .core
            .set_state(ids[0], DocState::Reading, Some("reading the scan"))
            .unwrap();
        assert_eq!(r.status_note.as_deref(), Some("reading the scan"));
        f.core
            .complete_processing(ids[0], &ex("abc"), &[], DocKind::unknown(), None)
            .unwrap();
        // Ready documents cannot be completed again.
        assert!(
            f.core
                .complete_processing(ids[0], &ex("abc"), &[], DocKind::unknown(), None)
                .is_err()
        );
        assert!(matches!(
            f.core.document(DocumentId::new()),
            Err(Error::UnknownHandle(_))
        ));
    }

    #[test]
    fn folder_ingest_resolve_and_summary() {
        let f = fx();
        let dir = f.root.join("Downloads/Taxes 2025");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("a.txt"), "a").unwrap();
        fs::write(dir.join("b.txt"), "b").unwrap();
        let ids = f
            .core
            .begin_ingest(std::slice::from_ref(&dir), IngestMode::Move)
            .unwrap();
        assert_eq!(ids.len(), 2);
        f.core
            .complete_processing(ids[0], &ex("a"), &[], DocKind("w2".into()), Some(2025))
            .unwrap();

        let list = f.core.list().unwrap();
        assert_eq!(list.len(), 1, "children are listed under their folder");
        assert!(list[0].is_folder);
        assert_eq!(list[0].handle.to_string(), "🐙/taxes-2025");
        assert_eq!(list[0].state, DocState::Ready);

        let Resolved::Folder(folder) = f.core.resolve(&h("taxes-2025")).unwrap() else {
            panic!()
        };
        assert_eq!(folder.children.len(), 2);
        let rec = f.core.document(ids[0]).unwrap();
        let fid = rec.folder.unwrap();
        let s = f.core.folder_summary(fid).unwrap();
        assert_eq!((s.total, s.ready, s.inking), (2, 1, 1));
        let desc = f.core.describe(&h("taxes-2025")).unwrap();
        assert_eq!(desc.kind.as_str(), "folder");

        // Writing against a folder handle lands in the folder.
        let out = f.core.write(&h("taxes-2025"), "plan.md", "notes").unwrap();
        assert_eq!(
            out.path,
            f.core.paths().private_folder.join("Taxes 2025/plan.md")
        );
        assert_eq!(out.handle.to_string(), "🐙/taxes-2025-plan");
        assert_eq!(f.core.find_document(&out.handle).unwrap().folder, Some(fid));

        // Re-ingesting the folder in place creates nothing new.
        let again = f
            .core
            .begin_ingest(
                &[f.core.paths().private_folder.join("Taxes 2025")],
                IngestMode::Move,
            )
            .unwrap();
        assert!(again.is_empty());
    }

    #[test]
    fn in_place_file_joins_its_folder() {
        let f = fx();
        let dir = f.root.join("Downloads/Box");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("a.txt"), "a").unwrap();
        f.core.begin_ingest(&[dir], IngestMode::Move).unwrap();
        let newfile = f.core.paths().private_folder.join("Box/new.txt");
        fs::write(&newfile, "n").unwrap();
        let ids = f.core.begin_ingest(&[newfile], IngestMode::Move).unwrap();
        assert!(f.core.document(ids[0]).unwrap().folder.is_some());
    }

    #[test]
    fn write_overwrites_own_output_but_not_originals() {
        let f = fx();
        let ids = f
            .core
            .begin_ingest(&[drop_file(&f, "w2.txt", "abc")], IngestMode::Move)
            .unwrap();
        f.core
            .complete_processing(ids[0], &ex("abc"), &[], DocKind("w2".into()), Some(2024))
            .unwrap();
        let handle = h("w2-2024");
        let a = f.core.write(&handle, "sum.md", "one").unwrap();
        let b = f.core.write(&handle, "sum.md", "two").unwrap();
        assert_eq!(a, b);
        assert_eq!(fs::read_to_string(&b.path).unwrap(), "two");
        assert!(matches!(
            f.core.write(&handle, "w2.txt", "clobber"),
            Err(Error::Refused(_))
        ));
        assert_eq!(
            fs::read_to_string(f.core.paths().private_folder.join("w2.txt")).unwrap(),
            "abc"
        );
        // A second output name gets its own handle.
        let c = f.core.write(&handle, "other.md", "x").unwrap();
        assert_eq!(c.handle.to_string(), "🐙/w2-2024-other");
    }

    #[test]
    fn unknown_handles() {
        let f = fx();
        assert!(matches!(
            f.core.resolve(&h("nope")),
            Err(Error::UnknownHandle(_))
        ));
        assert!(matches!(
            f.core.describe(&h("nope")),
            Err(Error::UnknownHandle(_))
        ));
        assert!(matches!(
            f.core.write(&h("nope"), "a", "b"),
            Err(Error::UnknownHandle(_))
        ));
        assert!(matches!(
            f.core.record_agent_read("x", &h("nope")),
            Err(Error::UnknownHandle(_))
        ));
        assert!(f.core.recent_reads(3).unwrap().is_empty());
    }
}

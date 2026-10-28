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
use crate::peek;
use crate::ports::{HostPaths, KeyStore};
use crate::tokenize::{self, Tokenized};
use crate::vault::{TokenScope, Vault, find_tokens};
use crate::{
    AgentRead, Description, Detection, DetectionSource, DocKind, DocState, DocumentId,
    DocumentRecord, Error, Extracted, FolderId, FolderRecord, FolderSummary, Handle, IngestMode,
    ListEntry, QuestionId, Resolved, ResolvedDocument, ResolvedFolder, ResolvedPage, Result,
    ReviewAnswer, ReviewQuestion, TokenKind, Treatment, WriteOutcome, ingest, rehydrate,
};

/// Kind given to files written by agents through `write`.
pub const OUTPUT_KIND: &str = "output";
/// Kind shown for folders in listings and `describe`.
pub const FOLDER_KIND: &str = "folder";

/// Roles whose organisation may name a document under
/// [`CodenamePolicy::TypeOrgYear`].
const NAMING_ROLES: &[&str] = &["employer", "payer", "lender"];

/// Name of the lock file in the data dir. Holding an exclusive advisory lock
/// on it makes the `Core` the vault's only writer.
pub const LOCK_FILE: &str = "inker.lock";

/// The platform-independent core. Thread-safe: share it behind an `Arc`.
#[derive(Debug)]
pub struct Core {
    paths: HostPaths,
    vault: Vault,
    index: Index,
    policy: CodenamePolicy,
    /// Exclusive lock on `<data_dir>/inker.lock`, released on drop.
    _lock: fs::File,
}

/// Restrict a directory to its owner (`0700`), whatever it was created with.
fn make_private_dir(dir: &Utf8Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(dir, fs::Permissions::from_mode(0o700))?;
    }
    #[cfg(not(unix))]
    let _ = dir;
    Ok(())
}

/// Take the data dir's writer lock without waiting.
fn lock_data_dir(dir: &Utf8Path) -> Result<fs::File> {
    let mut opts = fs::OpenOptions::new();
    opts.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let file = opts.open(dir.join(LOCK_FILE))?;
    match file.try_lock() {
        Ok(()) => Ok(file),
        Err(fs::TryLockError::WouldBlock) => Err(Error::Vault(
            "another INKER process holds this data dir".into(),
        )),
        Err(fs::TryLockError::Error(e)) => Err(Error::Io(e)),
    }
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
    source_text(ex, d)?.get(d.span.start..d.span.end)
}

/// The page text (or form field value) a detection's span indexes into.
fn source_text<'a>(ex: &'a Extracted, d: &Detection) -> Option<&'a str> {
    match &d.field {
        Some(name) => ex
            .fields
            .iter()
            .find(|f| &f.name == name)
            .map(|f| f.value.as_str()),
        None => ex
            .pages
            .iter()
            .find(|p| p.page == d.page)
            .map(|p| p.text.as_str()),
    }
}

/// The organisation that names the document under `TypeOrgYear`: only an Org
/// value read from a form template's employer, payer or lender box, and not
/// one held for review. Rules, models and free text never name a handle.
fn naming_org(ex: &Extracted, dets: &[Detection]) -> Option<String> {
    dets.iter()
        .filter(|d| {
            d.kind == TokenKind::Org
                && !d.needs_review
                && matches!(d.source, DetectionSource::Template { .. })
                && d.role.as_deref().is_some_and(|r| NAMING_ROLES.contains(&r))
        })
        .find_map(|d| detected_text(ex, d).map(str::to_string))
}

impl Core {
    /// Open the core: create the private folder and data dir if needed
    /// (the data dir `0700`), take the data dir's exclusive writer lock
    /// (failing fast with [`Error::Vault`] when another `Core` holds it),
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
        make_private_dir(&paths.data_dir)?;
        let lock = lock_data_dir(&paths.data_dir)?;
        let vault = Vault::open(&paths.data_dir.join("vault.bin"), keystore)?;
        let index = Index::open(&paths.data_dir.join("index.db"))?;
        Ok(Self {
            paths,
            vault,
            index,
            policy: CodenamePolicy::default(),
            _lock: lock,
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
        let scope = self.write_scope(handle)?;
        let (path, proposed, skipped_tokens) = rehydrate::write(
            &self.vault,
            &scope,
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
        tracing::info!(output = %rec.id.0, inked, "write");
        Ok(WriteOutcome {
            path,
            handle: Handle::new(rec.codename),
            skipped_tokens,
        })
    }

    /// Tokens a `write` against `handle` may expand: those in the source
    /// document's stored pages and fields, or for a folder those of its
    /// children. An output written by an agent is scoped to the document or
    /// folder it was written from (its own text is agent input), and outputs
    /// inside a folder do not count as its children.
    fn write_scope(&self, handle: &Handle) -> Result<TokenScope> {
        let mut target = self.lookup(handle)?;
        // Follow output -> source links to an original; bounded in case of a cycle.
        for _ in 0..64 {
            let Target::Doc(rec) = &target else { break };
            let Some(src) = self.index.output_source(rec.id)? else {
                break;
            };
            target = self.lookup(&Handle::new(src))?;
        }
        let mut scope = TokenScope::new();
        match target {
            Target::Doc(rec) => {
                if self.index.output_source(rec.id)?.is_none() {
                    self.add_doc_tokens(rec.id, &mut scope)?;
                }
            }
            Target::Folder(f) => {
                for child in self.index.folder_children(f.id)? {
                    if self.index.output_source(child.id)?.is_none() {
                        self.add_doc_tokens(child.id, &mut scope)?;
                    }
                }
            }
        }
        Ok(scope)
    }

    fn add_doc_tokens(&self, id: DocumentId, scope: &mut TokenScope) -> Result<()> {
        let pages = self.index.pages(id)?;
        let fields = self
            .index
            .derived(id)?
            .map(|d| d.fields)
            .unwrap_or_default();
        let texts = pages
            .iter()
            .map(|p| p.text.as_str())
            .chain(fields.iter().map(|f| f.value.as_str()));
        for text in texts {
            scope.extend(find_tokens(text).into_iter().map(|(_, l, i)| (l, i)));
        }
        Ok(())
    }

    /// Unanswered attention-card questions across all documents.
    /// The peek context (`context_before`, `context_after`) is unsealed here;
    /// the index only holds it sealed.
    pub fn pending_reviews(&self) -> Result<Vec<ReviewQuestion>> {
        let mut qs = self.index.questions(None, false)?;
        for q in &mut qs {
            if let Some(sealed) = self.index.question_context(q.id)? {
                let (before, after): (String, String) =
                    serde_json::from_slice(&self.vault.unseal(&sealed)?)?;
                q.context_before = before;
                q.context_after = after;
            }
        }
        Ok(qs)
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
        tracing::info!(doc = %rec.id.0, "review answered; ready");
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
        let base = codename::propose_folder(u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
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

    /// Every document whose state is one of `states`, oldest first.
    pub fn documents_in(&self, states: &[DocState]) -> Result<Vec<DocumentRecord>> {
        let mut docs = self.index.list_documents()?;
        docs.retain(|r| states.contains(&r.state));
        Ok(docs)
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
            tracing::info!(doc = %id.0, "ready");
            return Ok(DocState::Ready);
        }
        let sealed = self.vault.seal(&serde_json::to_vec(extracted)?)?;
        self.index.store_sealed_source(id, &sealed)?;
        let page_count = u32::try_from(extracted.pages.len()).unwrap_or(u32::MAX);
        for (i, d) in &review {
            let q = self
                .index
                .add_question(id, *i, d.page, &d.kind, d.review_prompt.as_deref())?;
            let value_length = detected_text(extracted, d)
                .map_or(0, |v| u32::try_from(v.chars().count()).unwrap_or(u32::MAX));
            let context = source_text(extracted, d)
                .map(|t| peek::context(t, d.span))
                .unwrap_or_default();
            let sealed = self.vault.seal(&serde_json::to_vec(&context)?)?;
            self.index
                .set_question_peek(q, value_length, page_count, &sealed)?;
        }
        self.index.set_state(id, DocState::NeedsAttention, None)?;
        tracing::info!(doc = %id.0, questions = review.len(), "needs attention");
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
    use crate::{Codename, PageText, Span, TextOrigin};
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
    fn question_carries_the_attention_card_peek() {
        let f = fx();
        let p1 = "Statement page one";
        let p2 = "Please wire to 00000000000 ref 4471 today";
        let ids = f
            .core
            .begin_ingest(
                &[drop_file(&f, "w.txt", &format!("{p1}\n{p2}"))],
                IngestMode::Move,
            )
            .unwrap();
        let extracted = Extracted {
            pages: vec![
                PageText {
                    page: 1,
                    text: p1.into(),
                    origin: TextOrigin::Ocr,
                },
                PageText {
                    page: 2,
                    text: p2.into(),
                    origin: TextOrigin::Ocr,
                },
            ],
            fields: vec![],
        };
        let mut det = d(p2, "00000000000", TokenKind::Account, true);
        det.page = 2;
        f.core
            .complete_processing(
                ids[0],
                &extracted,
                &[det],
                DocKind("statement".into()),
                None,
            )
            .unwrap();
        let qs = f.core.pending_reviews().unwrap();
        assert_eq!(qs.len(), 1);
        let q = &qs[0];
        assert_eq!(q.page, 2);
        assert_eq!(q.page_count, 2);
        assert_eq!(q.value_length, 11);
        assert!(
            q.context_before.ends_with("wire to "),
            "{:?}",
            q.context_before
        );
        assert!(q.context_after.starts_with(" ref"), "{:?}", q.context_after);
        assert!(!q.context_after.contains("4471"), "{:?}", q.context_after);
        let json = serde_json::to_string(q).unwrap();
        assert!(!json.contains("00000000000"), "{json}");
    }

    #[test]
    fn documents_in_lists_by_state() {
        let f = fx();
        let ids = f
            .core
            .begin_ingest(
                &[drop_file(&f, "a.txt", "a"), drop_file(&f, "b.txt", "b")],
                IngestMode::Move,
            )
            .unwrap();
        f.core.set_state(ids[1], DocState::Reading, None).unwrap();
        let reading = f.core.documents_in(&[DocState::Reading]).unwrap();
        assert_eq!(reading.iter().map(|r| r.id).collect::<Vec<_>>(), [ids[1]]);
        let both = f
            .core
            .documents_in(&[DocState::Queued, DocState::Reading])
            .unwrap();
        assert_eq!(both.len(), 2);
        assert!(f.core.documents_in(&[]).unwrap().is_empty());
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
        let fh = list[0].handle.clone();
        assert!(fh.codename.as_str().starts_with("folder-"), "{fh}");
        assert_eq!(list[0].state, DocState::Ready);

        let Resolved::Folder(folder) = f.core.resolve(&fh).unwrap() else {
            panic!()
        };
        assert_eq!(folder.children.len(), 2);
        let rec = f.core.document(ids[0]).unwrap();
        let fid = rec.folder.unwrap();
        let s = f.core.folder_summary(fid).unwrap();
        assert_eq!((s.total, s.ready, s.inking), (2, 1, 1));
        let desc = f.core.describe(&fh).unwrap();
        assert_eq!(desc.kind.as_str(), "folder");

        // Writing against a folder handle lands in the folder.
        let out = f.core.write(&fh, "plan.md", "notes").unwrap();
        assert_eq!(
            out.path,
            f.core.paths().private_folder.join("Taxes 2025/plan.md")
        );
        assert_eq!(
            out.handle.codename.as_str(),
            format!("{}-plan", fh.codename)
        );
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

    fn template(role: &str) -> DetectionSource {
        DetectionSource::Template {
            form: "f".into(),
            field: role.into(),
        }
    }

    fn org_det(text: &str, needle: &str, source: DetectionSource, role: &str) -> Detection {
        let mut x = d(text, needle, TokenKind::Org, false);
        x.source = source;
        x.role = Some(role.into());
        x
    }

    fn process(
        core: &Core,
        f: &Fx,
        name: &str,
        text: &str,
        dets: &[Detection],
        kind: &str,
    ) -> String {
        let ids = core
            .begin_ingest(&[drop_file_root(&f.root, name, text)], IngestMode::Move)
            .unwrap();
        core.complete_processing(ids[0], &ex(text), dets, DocKind(kind.into()), Some(2025))
            .unwrap();
        core.document(ids[0]).unwrap().codename.to_string()
    }

    #[test]
    fn default_policy_keeps_names_and_numbers_out_of_handles() {
        let f = fx();
        let text = "Payer Testy McFakename paid";
        let dets = [org_det(
            text,
            "Testy McFakename",
            template("payer"),
            "payer",
        )];
        assert_eq!(
            process(&f.core, &f, "a.txt", text, &dets, "1099-NEC"),
            "1099nec-2025"
        );
        assert_eq!(
            process(&f.core, &f, "b.txt", text, &dets, "1099-NEC"),
            "1099nec-2025-2"
        );
        assert_eq!(
            process(&f.core, &f, "c.txt", text, &dets, "1099-NEC"),
            "1099nec-2025-3"
        );
    }

    #[test]
    fn type_org_year_only_trusts_template_payer_orgs_and_drops_digits() {
        let Fx { _t, root, core } = fx();
        let core = core.with_codename_policy(CodenamePolicy::TypeOrgYear);
        let f = Fx { _t, root, core };
        let text = "Payer Acme 07-3456789 to Placeholder Bank";
        // A model guess never names the handle.
        let model = [org_det(
            text,
            "Placeholder Bank",
            DetectionSource::Model { name: "m".into() },
            "payer",
        )];
        assert_eq!(
            process(&f.core, &f, "a.txt", text, &model, "1099-INT"),
            "1099int-2025"
        );
        // A template org with a non-naming role does not either.
        let recip = [org_det(
            text,
            "Placeholder Bank",
            template("recipient"),
            "recipient",
        )];
        assert_eq!(
            process(&f.core, &f, "b.txt", text, &recip, "1099-INT"),
            "1099int-2025-2"
        );
        // A template payer does, without its digits.
        let payer = [org_det(text, "Acme 07-3456789", template("payer"), "payer")];
        assert_eq!(
            process(&f.core, &f, "c.txt", text, &payer, "1099-INT"),
            "1099int-acme-2025"
        );
        // A held (unsure) payer is not trusted.
        let mut held = org_det(text, "Placeholder Bank", template("payer"), "payer");
        held.needs_review = true;
        assert_eq!(
            process(&f.core, &f, "e.txt", text, &[held], "1099-INT"),
            "1099int-2025-3"
        );
    }

    #[test]
    fn folder_handles_never_carry_the_folder_name() {
        let f = fx();
        let dir = f.root.join("Downloads/Testy McFakename Taxes 2025");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("a.txt"), "a").unwrap();
        f.core.begin_ingest(&[dir], IngestMode::Move).unwrap();
        let list = f.core.list().unwrap();
        let code = list[0].handle.codename.as_str();
        assert!(code.starts_with("folder-"), "{code}");
        assert!(
            !code.contains("mcfakename") && !code.contains("taxes"),
            "{code}"
        );
    }

    /// Collects every field of every event, at every level.
    struct Capture(std::sync::Arc<std::sync::Mutex<String>>);

    struct Visit<'a>(&'a mut String);

    impl tracing::field::Visit for Visit<'_> {
        fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
            use std::fmt::Write;
            let _ = write!(self.0, " {}={:?}", field.name(), value);
        }
    }

    impl tracing::Subscriber for Capture {
        fn register_callsite(
            &self,
            _: &'static tracing::Metadata<'static>,
        ) -> tracing::subscriber::Interest {
            // Other tests run without a subscriber in parallel; never let a
            // cached "never" interest hide an event from this one.
            tracing::subscriber::Interest::sometimes()
        }
        fn enabled(&self, _: &tracing::Metadata<'_>) -> bool {
            true
        }
        fn new_span(&self, a: &tracing::span::Attributes<'_>) -> tracing::span::Id {
            a.record(&mut Visit(&mut self.0.lock().unwrap()));
            tracing::span::Id::from_u64(1)
        }
        fn record(&self, _: &tracing::span::Id, v: &tracing::span::Record<'_>) {
            v.record(&mut Visit(&mut self.0.lock().unwrap()));
        }
        fn record_follows_from(&self, _: &tracing::span::Id, _: &tracing::span::Id) {}
        fn event(&self, e: &tracing::Event<'_>) {
            let mut s = self.0.lock().unwrap();
            s.push('\n');
            e.record(&mut Visit(&mut s));
        }
        fn enter(&self, _: &tracing::span::Id) {}
        fn exit(&self, _: &tracing::span::Id) {}
    }

    #[test]
    fn logs_carry_no_codenames() {
        // Process-wide, so events from tests running in parallel land here
        // too; none of them may carry a codename either.
        let log = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
        tracing::subscriber::set_global_default(Capture(log.clone()))
            .expect("only this test installs a subscriber");
        let codes = {
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
            f.core
                .complete_processing(ids[0], &ex(text), &dets, DocKind("w2".into()), Some(2025))
                .unwrap();
            let q = f.core.pending_reviews().unwrap();
            f.core.answer_review(q[0].id, ReviewAnswer::InkIt).unwrap();
            let code = f.core.document(ids[0]).unwrap().codename;
            let out = f
                .core
                .write(&Handle::new(code.clone()), "sum.md", "[SSN_1]")
                .unwrap();
            vec![code.to_string(), out.handle.codename.to_string()]
        };
        let log = log.lock().unwrap();
        assert!(log.contains("ready"), "the capture works: {log}");
        for c in codes {
            assert!(!log.contains(&c), "{c} in logs: {log}");
        }
        assert!(!log.contains("000-12-3456") && !log.contains("98765432"));
    }

    #[test]
    fn second_open_on_the_same_data_dir_fails_fast() {
        let f = fx();
        let again = Core::open(f.core.paths().clone(), &MemoryKeyStore::new().unwrap());
        match again {
            Err(Error::Vault(m)) => assert_eq!(m, "another INKER process holds this data dir"),
            other => panic!("expected the lock to refuse, got {other:?}"),
        }
        // Released with the Core.
        let paths = f.core.paths().clone();
        drop(f.core);
        assert!(Core::open(paths, &MemoryKeyStore::new().unwrap()).is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn data_dir_and_index_files_are_private() {
        use std::os::unix::fs::PermissionsExt;
        let t = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(t.path().canonicalize().unwrap()).unwrap();
        let data = root.join("data");
        fs::create_dir_all(&data).unwrap();
        fs::set_permissions(&data, fs::Permissions::from_mode(0o755)).unwrap();
        let paths = HostPaths {
            private_folder: root.join("Private"),
            data_dir: data.clone(),
        };
        let core = Core::open(paths, &MemoryKeyStore::new().unwrap()).unwrap();
        let mode = |p: &Utf8Path| fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&data), 0o700);
        // Force the WAL and shared-memory files into existence.
        fs::create_dir_all(root.join("Downloads")).unwrap();
        let ids = core
            .begin_ingest(&[drop_file_root(&root, "a.txt", "abc")], IngestMode::Move)
            .unwrap();
        assert_eq!(ids.len(), 1);
        for name in ["index.db", "index.db-wal", "index.db-shm"] {
            let p = data.join(name);
            assert!(p.exists(), "{name}");
            assert_eq!(mode(&p), 0o600, "{name}");
        }
    }

    #[test]
    fn write_rehydrates_only_the_source_documents_tokens() {
        let f = fx();
        let a = "Employee SSN 000-12-3456";
        let b = "Other SSN 666-12-3456";
        let ca = process(
            &f.core,
            &f,
            "a.txt",
            a,
            &[d(a, "000-12-3456", TokenKind::Ssn, false)],
            "w2",
        );
        process(
            &f.core,
            &f,
            "b.txt",
            b,
            &[d(b, "666-12-3456", TokenKind::Ssn, false)],
            "w2",
        );
        let ha = h(&ca);
        let out = f
            .core
            .write(&ha, "dump.md", "[SSN_1] [SSN_2] [SSN_500]")
            .unwrap();
        assert_eq!(
            fs::read_to_string(&out.path).unwrap(),
            "000-12-3456 [SSN_2] [SSN_500]"
        );
        assert_eq!(out.skipped_tokens, 2);
        // The output's own tokenized view holds [SSN_2] literally; writing
        // against the output must not widen the scope to it.
        let again = f
            .core
            .write(&out.handle, "dump2.md", "[SSN_1] [SSN_2]")
            .unwrap();
        assert_eq!(
            fs::read_to_string(&again.path).unwrap(),
            "000-12-3456 [SSN_2]"
        );
        assert_eq!(again.skipped_tokens, 1);
    }

    #[test]
    fn folder_write_scope_is_its_children() {
        let f = fx();
        let dir = f.root.join("Downloads/Box");
        fs::create_dir_all(&dir).unwrap();
        let a = "SSN 000-12-3456";
        fs::write(dir.join("a.txt"), a).unwrap();
        let ids = f.core.begin_ingest(&[dir], IngestMode::Move).unwrap();
        f.core
            .complete_processing(
                ids[0],
                &ex(a),
                &[d(a, "000-12-3456", TokenKind::Ssn, false)],
                DocKind("w2".into()),
                Some(2025),
            )
            .unwrap();
        let b = "SSN 666-12-3456";
        process(
            &f.core,
            &f,
            "b.txt",
            b,
            &[d(b, "666-12-3456", TokenKind::Ssn, false)],
            "w2",
        );
        let fh = f
            .core
            .list()
            .unwrap()
            .into_iter()
            .find(|e| e.is_folder)
            .unwrap()
            .handle;
        let out = f.core.write(&fh, "plan.md", "[SSN_1] [SSN_2]").unwrap();
        assert_eq!(
            fs::read_to_string(&out.path).unwrap(),
            "000-12-3456 [SSN_2]"
        );
        assert_eq!(out.skipped_tokens, 1);
        // An output inside the folder does not widen the folder's scope.
        let out2 = f.core.write(&fh, "plan2.md", "[SSN_2]").unwrap();
        assert_eq!(out2.skipped_tokens, 1);
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

//! The index: a SQLite store at `<data_dir>/index.db`.
//!
//! Everything derived from a document lives here, never beside the file (no
//! mirror folder): records, tokenized pages, treated form fields, the role
//! map, detections (spans and kinds only), review questions, the agent read
//! log, and, for held documents, the extracted text sealed with the vault key
//! so it can be re-inked after the user answers. No plaintext real value is
//! ever written to this database.
//!
//! WAL mode; migrations are tracked in a `schema_version` table.

use std::fmt;
use std::sync::{Mutex, MutexGuard};

use camino::{Utf8Path, Utf8PathBuf};
use chrono::{DateTime, SecondsFormat, Utc};
use rusqlite::types::Type;
use rusqlite::{Connection, OptionalExtension, Row, params};
use serde::Serialize;
use serde::de::DeserializeOwned;
use uuid::Uuid;

use crate::{
    AgentRead, Codename, Detection, DocKind, DocState, DocumentId, DocumentRecord, Error, FolderId,
    FolderRecord, FolderSummary, FormField, Handle, KindTreatment, QuestionId, ResolvedPage,
    Result, ReviewAnswer, ReviewQuestion, Span, Token, TokenKind, TokenRole,
};

/// Current schema version.
pub const SCHEMA_VERSION: u32 = 4;

const MIGRATIONS: &[&str] = &[
    // 1
    r#"
    CREATE TABLE folders (
        id          TEXT PRIMARY KEY,
        codename    TEXT NOT NULL UNIQUE,
        path        TEXT NOT NULL UNIQUE,
        created_at  TEXT NOT NULL
    );
    CREATE TABLE documents (
        id            TEXT PRIMARY KEY,
        codename      TEXT NOT NULL UNIQUE,
        path          TEXT NOT NULL,
        original_name TEXT NOT NULL,
        kind          TEXT NOT NULL,
        year          INTEGER,
        state         TEXT NOT NULL,
        inked         INTEGER NOT NULL DEFAULT 0,
        folder_id     TEXT REFERENCES folders(id),
        ingest        TEXT NOT NULL,
        created_at    TEXT NOT NULL,
        updated_at    TEXT NOT NULL,
        status_note   TEXT,
        provisional   INTEGER NOT NULL DEFAULT 0
    );
    CREATE INDEX documents_path ON documents(path);
    CREATE INDEX documents_folder ON documents(folder_id);
    CREATE TABLE pages (
        doc_id  TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
        page    INTEGER NOT NULL,
        text    TEXT NOT NULL,
        PRIMARY KEY (doc_id, page)
    );
    CREATE TABLE derived (
        doc_id      TEXT PRIMARY KEY REFERENCES documents(id) ON DELETE CASCADE,
        fields      TEXT NOT NULL,
        roles       TEXT NOT NULL,
        treatments  TEXT NOT NULL
    );
    CREATE TABLE detections (
        doc_id  TEXT PRIMARY KEY REFERENCES documents(id) ON DELETE CASCADE,
        json    TEXT NOT NULL
    );
    CREATE TABLE sealed_sources (
        doc_id  TEXT PRIMARY KEY REFERENCES documents(id) ON DELETE CASCADE,
        blob    BLOB NOT NULL
    );
    CREATE TABLE review_questions (
        id              INTEGER PRIMARY KEY AUTOINCREMENT,
        doc_id          TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
        detection_index INTEGER NOT NULL,
        page            INTEGER NOT NULL,
        kind            TEXT NOT NULL,
        prompt          TEXT,
        answer          TEXT,
        created_at      TEXT NOT NULL,
        answered_at     TEXT
    );
    CREATE INDEX review_questions_doc ON review_questions(doc_id);
    CREATE TABLE agent_reads (
        id        INTEGER PRIMARY KEY AUTOINCREMENT,
        agent     TEXT NOT NULL,
        codename  TEXT NOT NULL,
        at        TEXT NOT NULL
    );
    CREATE TABLE outputs (
        doc_id     TEXT PRIMARY KEY REFERENCES documents(id) ON DELETE CASCADE,
        source     TEXT NOT NULL
    );
    "#,
    // 2: the attention card's peek. The context is sealed with the vault key,
    // because the words near a held value can be personal.
    r#"
    ALTER TABLE review_questions ADD COLUMN value_length INTEGER NOT NULL DEFAULT 0;
    ALTER TABLE review_questions ADD COLUMN page_count INTEGER NOT NULL DEFAULT 0;
    ALTER TABLE review_questions ADD COLUMN context BLOB;
    "#,
    // 3: finding documents. The tag chip and the payer's token (never the
    // payer's name: that stays in the vault).
    r#"
    ALTER TABLE documents ADD COLUMN tag TEXT;
    ALTER TABLE documents ADD COLUMN payer_token TEXT;
    "#,
    // 4: the attention card's page crop. Where the held value sits in its
    // page's text; a byte range, never the value.
    r#"
    ALTER TABLE review_questions ADD COLUMN span_start INTEGER;
    ALTER TABLE review_questions ADD COLUMN span_end INTEGER;
    "#,
];

const DOC_COLS: &str = "id, codename, path, original_name, kind, year, state, inked, folder_id, ingest, \
                        created_at, updated_at, status_note, tag, payer_token";
const FOLDER_COLS: &str = "id, codename, path, created_at";
const Q_COLS: &str = "id, doc_id, page, kind, prompt, answer, created_at, answered_at, value_length, \
                      page_count, span_start, span_end";

/// Treated form fields, role map and treatment counts for one document.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DerivedData {
    pub fields: Vec<FormField>,
    pub roles: Vec<TokenRole>,
    pub treatments: Vec<KindTreatment>,
}

impl From<rusqlite::Error> for Error {
    fn from(e: rusqlite::Error) -> Self {
        Error::Index(e.to_string())
    }
}

/// The SQLite index. Thread-safe; share it behind `&`.
pub struct Index {
    conn: Mutex<Connection>,
}

impl fmt::Debug for Index {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Index").finish_non_exhaustive()
    }
}

// --- value conversions -----------------------------------------------------

fn ts(t: DateTime<Utc>) -> String {
    t.to_rfc3339_opts(SecondsFormat::Micros, true)
}

fn parse_ts(s: &str) -> Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .map(|d| d.with_timezone(&Utc))
        .map_err(|e| Error::Index(format!("bad timestamp: {e}")))
}

/// Unit enums as their bare snake_case name.
fn enum_str<T: Serialize>(v: &T) -> Result<String> {
    Ok(match serde_json::to_value(v)? {
        serde_json::Value::String(s) => s,
        other => other.to_string(),
    })
}

fn enum_parse<T: DeserializeOwned>(s: &str) -> Result<T> {
    serde_json::from_value(serde_json::Value::String(s.to_string()))
        .or_else(|_| serde_json::from_str(s))
        .map_err(|e| Error::Index(format!("bad enum value: {e}")))
}

fn uuid(s: &str) -> Result<Uuid> {
    Uuid::parse_str(s).map_err(|e| Error::Index(format!("bad id: {e}")))
}

fn codename(s: &str) -> Result<Codename> {
    Codename::parse(s).map_err(Error::Index)
}

/// Lift a core error into a rusqlite row-mapping error.
fn conv(idx: usize, e: Error) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(idx, Type::Text, Box::new(e))
}

/// An integer column that does not fit the field it feeds.
fn int_conv(idx: usize, e: std::num::TryFromIntError) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(idx, Type::Integer, Box::new(e))
}

fn doc_from_row(row: &Row<'_>) -> rusqlite::Result<DocumentRecord> {
    let s = |i: usize| row.get::<_, String>(i);
    let year: Option<i64> = row.get(5)?;
    let folder: Option<String> = row.get(8)?;
    Ok(DocumentRecord {
        id: DocumentId(uuid(&s(0)?).map_err(|e| conv(0, e))?),
        codename: codename(&s(1)?).map_err(|e| conv(1, e))?,
        path: Utf8PathBuf::from(s(2)?),
        original_name: s(3)?,
        kind: DocKind(s(4)?),
        year: year.and_then(|y| u16::try_from(y).ok()),
        state: enum_parse(&s(6)?).map_err(|e| conv(6, e))?,
        inked: row.get(7)?,
        folder: match folder {
            Some(f) => Some(FolderId(uuid(&f).map_err(|e| conv(8, e))?)),
            None => None,
        },
        ingest: enum_parse(&s(9)?).map_err(|e| conv(9, e))?,
        created_at: parse_ts(&s(10)?).map_err(|e| conv(10, e))?,
        updated_at: parse_ts(&s(11)?).map_err(|e| conv(11, e))?,
        status_note: row.get(12)?,
        tag: row.get(13)?,
        payer_token: row
            .get::<_, Option<String>>(14)?
            .and_then(|t| parse_token(&t)),
    })
}

/// `[ORG_3]` back to a token; `None` for anything else.
fn parse_token(s: &str) -> Option<Token> {
    let (range, label, index) = crate::vault::find_tokens(s).into_iter().next()?;
    if range != (0..s.len()) {
        return None;
    }
    Some(Token {
        kind: token_kind_from_label(&label),
        index,
    })
}

/// Inverse of [`TokenKind::label`] for the built-in kinds; anything else is
/// a custom kind with that label.
fn token_kind_from_label(label: &str) -> TokenKind {
    match label {
        "SSN" => TokenKind::Ssn,
        "ITIN" => TokenKind::Itin,
        "EIN" => TokenKind::Ein,
        "ACCOUNT" => TokenKind::Account,
        "ROUTING" => TokenKind::Routing,
        "CARD" => TokenKind::Card,
        "PERSON" => TokenKind::Person,
        "ORG" => TokenKind::Org,
        "ADDRESS" => TokenKind::Address,
        "EMAIL" => TokenKind::Email,
        "PHONE" => TokenKind::Phone,
        "GOVID" => TokenKind::GovId,
        other => TokenKind::Custom(other.to_string()),
    }
}

fn folder_from_row(row: &Row<'_>) -> rusqlite::Result<FolderRecord> {
    let s = |i: usize| row.get::<_, String>(i);
    Ok(FolderRecord {
        id: FolderId(uuid(&s(0)?).map_err(|e| conv(0, e))?),
        codename: codename(&s(1)?).map_err(|e| conv(1, e))?,
        path: Utf8PathBuf::from(s(2)?),
        created_at: parse_ts(&s(3)?).map_err(|e| conv(3, e))?,
    })
}

fn question_from_row(row: &Row<'_>) -> rusqlite::Result<ReviewQuestion> {
    let s = |i: usize| row.get::<_, String>(i);
    let answer: Option<String> = row.get(5)?;
    let answered_at: Option<String> = row.get(7)?;
    Ok(ReviewQuestion {
        id: QuestionId(row.get(0)?),
        document: DocumentId(uuid(&s(1)?).map_err(|e| conv(1, e))?),
        page: row.get(2)?,
        kind: serde_json::from_str(&s(3)?).map_err(|e| conv(3, e.into()))?,
        prompt: row.get(4)?,
        answer: match answer {
            Some(a) => Some(enum_parse(&a).map_err(|e| conv(5, e))?),
            None => None,
        },
        created_at: parse_ts(&s(6)?).map_err(|e| conv(6, e))?,
        answered_at: match answered_at {
            Some(a) => Some(parse_ts(&a).map_err(|e| conv(7, e))?),
            None => None,
        },
        value_length: row.get(8)?,
        context_before: String::new(),
        context_after: String::new(),
        page_count: row.get(9)?,
        span: match (
            row.get::<_, Option<i64>>(10)?,
            row.get::<_, Option<i64>>(11)?,
        ) {
            (Some(start), Some(end)) => Some(Span {
                start: usize::try_from(start).map_err(|e| int_conv(10, e))?,
                end: usize::try_from(end).map_err(|e| int_conv(11, e))?,
            }),
            _ => None,
        },
    })
}

/// Create `path` as an empty `0600` file, or chmod an existing one to `0600`.
#[cfg(unix)]
fn restrict(path: &Utf8Path) -> Result<()> {
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(path)?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    Ok(())
}

impl Index {
    /// Open or create the index at `path`, enable WAL and run migrations.
    pub fn open(path: &Utf8Path) -> Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        // SQLite gives the -wal and -shm files the database file's mode, so
        // making index.db 0600 before the first open covers all three; the
        // chmods after cover files left by an older build.
        #[cfg(unix)]
        restrict(path)?;
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        let index = Self::init(conn)?;
        #[cfg(unix)]
        for suffix in ["-wal", "-shm"] {
            let side = Utf8PathBuf::from(format!("{path}{suffix}"));
            if side.exists() {
                restrict(&side)?;
            }
        }
        Ok(index)
    }

    /// A throwaway in-memory index (tests).
    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(mut conn: Connection) -> Result<Self> {
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        let tx = conn.transaction()?;
        tx.execute_batch("CREATE TABLE IF NOT EXISTS schema_version (version INTEGER NOT NULL);")?;
        let current: u32 = tx.query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_version",
            [],
            |r| r.get(0),
        )?;
        for (i, sql) in MIGRATIONS.iter().enumerate() {
            let version = u32::try_from(i + 1).unwrap_or(u32::MAX);
            if version > current {
                tx.execute_batch(sql)?;
                tx.execute(
                    "INSERT INTO schema_version (version) VALUES (?1)",
                    [version],
                )?;
                tracing::info!(version, "index migrated");
            }
        }
        tx.commit()?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    fn conn(&self) -> MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn schema_version(&self) -> Result<u32> {
        Ok(self
            .conn()
            .query_row("SELECT MAX(version) FROM schema_version", [], |r| r.get(0))?)
    }

    pub fn journal_mode(&self) -> Result<String> {
        Ok(self
            .conn()
            .query_row("PRAGMA journal_mode", [], |r| r.get(0))?)
    }

    // --- documents ---------------------------------------------------------

    /// Insert a new record. `provisional` marks a placeholder codename that
    /// [`Index::set_codename`] will replace once the document is classified.
    pub fn insert_document(&self, r: &DocumentRecord, provisional: bool) -> Result<()> {
        self.conn().execute(
            &format!("INSERT INTO documents ({DOC_COLS}, provisional) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16)"),
            params![
                r.id.0.to_string(),
                r.codename.as_str(),
                r.path.as_str(),
                r.original_name,
                r.kind.as_str(),
                r.year,
                enum_str(&r.state)?,
                r.inked,
                r.folder.map(|f| f.0.to_string()),
                enum_str(&r.ingest)?,
                ts(r.created_at),
                ts(r.updated_at),
                r.status_note,
                r.tag,
                r.payer_token.as_ref().map(ToString::to_string),
                provisional,
            ],
        )?;
        Ok(())
    }

    /// Replace a record's fields. A state change must be a legal transition.
    /// `updated_at` is set to now.
    pub fn update_document(&self, r: &DocumentRecord) -> Result<()> {
        let current = self
            .document(r.id)?
            .ok_or_else(|| Error::UnknownHandle(r.codename.to_string()))?;
        check_transition(current.state, r.state)?;
        self.conn().execute(
            "UPDATE documents SET codename=?2, path=?3, original_name=?4, kind=?5, year=?6, state=?7, inked=?8, \
             folder_id=?9, ingest=?10, updated_at=?11, status_note=?12, tag=?13, payer_token=?14 WHERE id=?1",
            params![
                r.id.0.to_string(),
                r.codename.as_str(),
                r.path.as_str(),
                r.original_name,
                r.kind.as_str(),
                r.year,
                enum_str(&r.state)?,
                r.inked,
                r.folder.map(|f| f.0.to_string()),
                enum_str(&r.ingest)?,
                ts(Utc::now()),
                r.status_note,
                r.tag,
                r.payer_token.as_ref().map(ToString::to_string),
            ],
        )?;
        Ok(())
    }

    /// Set the final codename and clear the provisional flag.
    pub fn set_codename(&self, id: DocumentId, c: &Codename) -> Result<()> {
        let n = self.conn().execute(
            "UPDATE documents SET codename=?2, provisional=0, updated_at=?3 WHERE id=?1",
            params![id.0.to_string(), c.as_str(), ts(Utc::now())],
        )?;
        if n == 0 {
            return Err(Error::UnknownHandle(id.0.to_string()));
        }
        Ok(())
    }

    pub fn is_provisional(&self, id: DocumentId) -> Result<bool> {
        self.conn()
            .query_row(
                "SELECT provisional FROM documents WHERE id=?1",
                [id.0.to_string()],
                |r| r.get(0),
            )
            .optional()?
            .ok_or_else(|| Error::UnknownHandle(id.0.to_string()))
    }

    /// Move a document through the state machine; `note` replaces the status
    /// note (None clears it). Returns the updated record.
    pub fn set_state(
        &self,
        id: DocumentId,
        to: DocState,
        note: Option<&str>,
    ) -> Result<DocumentRecord> {
        let mut r = self
            .document(id)?
            .ok_or_else(|| Error::UnknownHandle(id.0.to_string()))?;
        check_transition(r.state, to)?;
        let now = Utc::now();
        self.conn().execute(
            "UPDATE documents SET state=?2, status_note=?3, updated_at=?4 WHERE id=?1",
            params![id.0.to_string(), enum_str(&to)?, note, ts(now)],
        )?;
        tracing::debug!(doc = %id.0, from = ?r.state, ?to, "state");
        r.state = to;
        r.status_note = note.map(str::to_string);
        r.updated_at = parse_ts(&ts(now))?;
        Ok(r)
    }

    fn query_docs(
        &self,
        where_clause: &str,
        p: impl rusqlite::Params,
    ) -> Result<Vec<DocumentRecord>> {
        let conn = self.conn();
        let mut st = conn.prepare(&format!("SELECT {DOC_COLS} FROM documents {where_clause}"))?;
        let rows = st
            .query_map(p, doc_from_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn document(&self, id: DocumentId) -> Result<Option<DocumentRecord>> {
        Ok(self.query_docs("WHERE id=?1", [id.0.to_string()])?.pop())
    }

    pub fn find_by_codename(&self, c: &Codename) -> Result<Option<DocumentRecord>> {
        Ok(self.query_docs("WHERE codename=?1", [c.as_str()])?.pop())
    }

    pub fn find_by_path(&self, p: &Utf8Path) -> Result<Option<DocumentRecord>> {
        Ok(self.query_docs("WHERE path=?1", [p.as_str()])?.pop())
    }

    /// All documents, oldest first.
    pub fn list_documents(&self) -> Result<Vec<DocumentRecord>> {
        self.query_docs("ORDER BY created_at, codename", [])
    }

    // --- folders -----------------------------------------------------------

    pub fn insert_folder(&self, f: &FolderRecord) -> Result<()> {
        self.conn().execute(
            &format!("INSERT INTO folders ({FOLDER_COLS}) VALUES (?1,?2,?3,?4)"),
            params![
                f.id.0.to_string(),
                f.codename.as_str(),
                f.path.as_str(),
                ts(f.created_at)
            ],
        )?;
        Ok(())
    }

    fn query_folders(
        &self,
        where_clause: &str,
        p: impl rusqlite::Params,
    ) -> Result<Vec<FolderRecord>> {
        let conn = self.conn();
        let mut st = conn.prepare(&format!("SELECT {FOLDER_COLS} FROM folders {where_clause}"))?;
        let rows = st
            .query_map(p, folder_from_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn folder(&self, id: FolderId) -> Result<Option<FolderRecord>> {
        Ok(self.query_folders("WHERE id=?1", [id.0.to_string()])?.pop())
    }

    pub fn find_folder_by_codename(&self, c: &Codename) -> Result<Option<FolderRecord>> {
        Ok(self.query_folders("WHERE codename=?1", [c.as_str()])?.pop())
    }

    pub fn find_folder_by_path(&self, p: &Utf8Path) -> Result<Option<FolderRecord>> {
        Ok(self.query_folders("WHERE path=?1", [p.as_str()])?.pop())
    }

    pub fn list_folders(&self) -> Result<Vec<FolderRecord>> {
        self.query_folders("ORDER BY created_at, codename", [])
    }

    pub fn folder_children(&self, id: FolderId) -> Result<Vec<DocumentRecord>> {
        self.query_docs("WHERE folder_id=?1 ORDER BY path", [id.0.to_string()])
    }

    /// "4 files · 1 ready · 3 inking". Queued, reading and inking all count as inking.
    pub fn folder_summary(&self, id: FolderId) -> Result<FolderSummary> {
        let mut s = FolderSummary::default();
        for d in self.folder_children(id)? {
            s.total += 1;
            s.inked += d.inked;
            match d.state {
                DocState::Ready => s.ready += 1,
                DocState::Queued | DocState::Reading | DocState::Inking => s.inking += 1,
                DocState::NeedsAttention => s.needs_attention += 1,
                DocState::Failed => s.failed += 1,
            }
        }
        Ok(s)
    }

    /// Whether a document or folder already uses this codename.
    pub fn codename_taken(&self, c: &str) -> Result<bool> {
        Ok(self.conn().query_row(
            "SELECT EXISTS(SELECT 1 FROM documents WHERE codename=?1) OR EXISTS(SELECT 1 FROM folders WHERE codename=?1)",
            [c],
            |r| r.get(0),
        )?)
    }

    // --- derived content ---------------------------------------------------

    /// Replace the tokenized pages of a document.
    pub fn store_pages(&self, id: DocumentId, pages: &[ResolvedPage]) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM pages WHERE doc_id=?1", [id.0.to_string()])?;
        for p in pages {
            tx.execute(
                "INSERT INTO pages (doc_id, page, text) VALUES (?1,?2,?3)",
                params![id.0.to_string(), p.page, p.text],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn pages(&self, id: DocumentId) -> Result<Vec<ResolvedPage>> {
        let conn = self.conn();
        let mut st = conn.prepare("SELECT page, text FROM pages WHERE doc_id=?1 ORDER BY page")?;
        let rows = st
            .query_map([id.0.to_string()], |r| {
                Ok(ResolvedPage {
                    page: r.get(0)?,
                    text: r.get(1)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn store_derived(&self, id: DocumentId, d: &DerivedData) -> Result<()> {
        self.conn().execute(
            "INSERT OR REPLACE INTO derived (doc_id, fields, roles, treatments) VALUES (?1,?2,?3,?4)",
            params![
                id.0.to_string(),
                serde_json::to_string(&d.fields)?,
                serde_json::to_string(&d.roles)?,
                serde_json::to_string(&d.treatments)?,
            ],
        )?;
        Ok(())
    }

    pub fn derived(&self, id: DocumentId) -> Result<Option<DerivedData>> {
        let row: Option<(String, String, String)> = self
            .conn()
            .query_row(
                "SELECT fields, roles, treatments FROM derived WHERE doc_id=?1",
                [id.0.to_string()],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;
        match row {
            None => Ok(None),
            Some((f, r, t)) => Ok(Some(DerivedData {
                fields: serde_json::from_str(&f)?,
                roles: serde_json::from_str(&r)?,
                treatments: serde_json::from_str(&t)?,
            })),
        }
    }

    /// Detections carry spans, kinds and roles, never values.
    pub fn store_detections(&self, id: DocumentId, d: &[Detection]) -> Result<()> {
        self.conn().execute(
            "INSERT OR REPLACE INTO detections (doc_id, json) VALUES (?1,?2)",
            params![id.0.to_string(), serde_json::to_string(d)?],
        )?;
        Ok(())
    }

    pub fn detections(&self, id: DocumentId) -> Result<Vec<Detection>> {
        let json: Option<String> = self
            .conn()
            .query_row(
                "SELECT json FROM detections WHERE doc_id=?1",
                [id.0.to_string()],
                |r| r.get(0),
            )
            .optional()?;
        Ok(match json {
            Some(j) => serde_json::from_str(&j)?,
            None => Vec::new(),
        })
    }

    /// Store a blob already sealed with the vault key (never plaintext).
    pub fn store_sealed_source(&self, id: DocumentId, blob: &[u8]) -> Result<()> {
        self.conn().execute(
            "INSERT OR REPLACE INTO sealed_sources (doc_id, blob) VALUES (?1,?2)",
            params![id.0.to_string(), blob],
        )?;
        Ok(())
    }

    pub fn sealed_source(&self, id: DocumentId) -> Result<Option<Vec<u8>>> {
        Ok(self
            .conn()
            .query_row(
                "SELECT blob FROM sealed_sources WHERE doc_id=?1",
                [id.0.to_string()],
                |r| r.get(0),
            )
            .optional()?)
    }

    pub fn delete_sealed_source(&self, id: DocumentId) -> Result<()> {
        self.conn().execute(
            "DELETE FROM sealed_sources WHERE doc_id=?1",
            [id.0.to_string()],
        )?;
        Ok(())
    }

    // --- review questions --------------------------------------------------

    /// Record an attention-card question about detection `detection_index`.
    pub fn add_question(
        &self,
        doc: DocumentId,
        detection_index: usize,
        page: u32,
        kind: &TokenKind,
        prompt: Option<&str>,
    ) -> Result<QuestionId> {
        let conn = self.conn();
        conn.execute(
            "INSERT INTO review_questions (doc_id, detection_index, page, kind, prompt, created_at) \
             VALUES (?1,?2,?3,?4,?5,?6)",
            params![
                doc.0.to_string(),
                i64::try_from(detection_index).unwrap_or(i64::MAX),
                page,
                serde_json::to_string(kind)?,
                prompt,
                ts(Utc::now()),
            ],
        )?;
        Ok(QuestionId(conn.last_insert_rowid()))
    }

    /// Record what the attention card shows around a question's held value:
    /// its length, the document's page count, where it sits in the page's
    /// text (`None` for a form field), and the context, which the caller
    /// has sealed.
    pub fn set_question_peek(
        &self,
        id: QuestionId,
        value_length: u32,
        page_count: u32,
        span: Option<Span>,
        sealed_context: &[u8],
    ) -> Result<()> {
        let start = span.map(|s| i64::try_from(s.start).unwrap_or(i64::MAX));
        let end = span.map(|s| i64::try_from(s.end).unwrap_or(i64::MAX));
        self.conn().execute(
            "UPDATE review_questions SET value_length=?2, page_count=?3, span_start=?4, span_end=?5, \
             context=?6 WHERE id=?1",
            params![id.0, value_length, page_count, start, end, sealed_context],
        )?;
        Ok(())
    }

    /// The sealed context of a question, if one was recorded.
    pub fn question_context(&self, id: QuestionId) -> Result<Option<Vec<u8>>> {
        Ok(self
            .conn()
            .query_row(
                "SELECT context FROM review_questions WHERE id=?1",
                [id.0],
                |r| r.get::<_, Option<Vec<u8>>>(0),
            )
            .optional()?
            .flatten())
    }

    /// Questions for one document (or all), oldest first.
    pub fn questions(
        &self,
        doc: Option<DocumentId>,
        include_answered: bool,
    ) -> Result<Vec<ReviewQuestion>> {
        let conn = self.conn();
        let mut sql =
            format!("SELECT {Q_COLS} FROM review_questions WHERE (?1 IS NULL OR doc_id=?1)");
        if !include_answered {
            sql.push_str(" AND answer IS NULL");
        }
        sql.push_str(" ORDER BY id");
        let mut st = conn.prepare(&sql)?;
        let rows = st
            .query_map([doc.map(|d| d.0.to_string())], question_from_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn question(&self, id: QuestionId) -> Result<Option<ReviewQuestion>> {
        Ok(self
            .conn()
            .query_row(
                &format!("SELECT {Q_COLS} FROM review_questions WHERE id=?1"),
                [id.0],
                question_from_row,
            )
            .optional()?)
    }

    /// Record the user's answer. Each question is answered once.
    pub fn answer_question(&self, id: QuestionId, answer: ReviewAnswer) -> Result<ReviewQuestion> {
        let n = self.conn().execute(
            "UPDATE review_questions SET answer=?2, answered_at=?3, context=NULL \
             WHERE id=?1 AND answer IS NULL",
            params![id.0, enum_str(&answer)?, ts(Utc::now())],
        )?;
        if n == 0 {
            return Err(Error::Index(format!(
                "question {} does not exist or is already answered",
                id.0
            )));
        }
        self.question(id)?
            .ok_or_else(|| Error::Index(format!("question {} vanished", id.0)))
    }

    /// (detection index, answer) for every answered question of a document.
    pub fn answered_detections(&self, doc: DocumentId) -> Result<Vec<(usize, ReviewAnswer)>> {
        let conn = self.conn();
        let mut st = conn.prepare(
            "SELECT detection_index, answer FROM review_questions \
             WHERE doc_id=?1 AND answer IS NOT NULL ORDER BY detection_index",
        )?;
        let rows = st
            .query_map([doc.0.to_string()], |r| {
                let i: i64 = r.get(0)?;
                let a: String = r.get(1)?;
                Ok((
                    usize::try_from(i).unwrap_or(usize::MAX),
                    enum_parse(&a).map_err(|e| conv(1, e))?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn clear_questions(&self, doc: DocumentId) -> Result<()> {
        self.conn().execute(
            "DELETE FROM review_questions WHERE doc_id=?1",
            [doc.0.to_string()],
        )?;
        Ok(())
    }

    // --- audit log and outputs ---------------------------------------------

    pub fn record_agent_read(&self, agent: &str, c: &Codename) -> Result<AgentRead> {
        let at = parse_ts(&ts(Utc::now()))?;
        self.conn().execute(
            "INSERT INTO agent_reads (agent, codename, at) VALUES (?1,?2,?3)",
            params![agent, c.as_str(), ts(at)],
        )?;
        Ok(AgentRead {
            agent: agent.to_string(),
            handle: Handle::new(c.clone()),
            at,
        })
    }

    /// The `n` most recent reads, newest first.
    pub fn recent_reads(&self, n: usize) -> Result<Vec<AgentRead>> {
        let conn = self.conn();
        let mut st =
            conn.prepare("SELECT agent, codename, at FROM agent_reads ORDER BY id DESC LIMIT ?1")?;
        let rows = st
            .query_map([i64::try_from(n).unwrap_or(i64::MAX)], |r| {
                let code: String = r.get(1)?;
                let at: String = r.get(2)?;
                Ok(AgentRead {
                    agent: r.get(0)?,
                    handle: Handle::new(codename(&code).map_err(|e| conv(1, e))?),
                    at: parse_ts(&at).map_err(|e| conv(2, e))?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// The most recent read of one codename, if any.
    pub fn last_read(&self, c: &Codename) -> Result<Option<AgentRead>> {
        let conn = self.conn();
        let mut st = conn.prepare(
            "SELECT agent, codename, at FROM agent_reads WHERE codename=?1 ORDER BY id DESC LIMIT 1",
        )?;
        let row = st
            .query_row([c.as_str()], |r| {
                let code: String = r.get(1)?;
                let at: String = r.get(2)?;
                Ok(AgentRead {
                    agent: r.get(0)?,
                    handle: Handle::new(codename(&code).map_err(|e| conv(1, e))?),
                    at: parse_ts(&at).map_err(|e| conv(2, e))?,
                })
            })
            .optional()?;
        Ok(row)
    }

    /// Documents whose tokenized text contains `needle` (case-insensitive
    /// for ASCII; the caller confirms the hit on the returned text), with the
    /// first matching page's text. Never a real value: pages hold tokens.
    pub fn search_pages(&self, needle: &str) -> Result<Vec<(DocumentId, String)>> {
        let pattern = format!(
            "%{}%",
            needle
                .replace('\\', "\\\\")
                .replace('%', "\\%")
                .replace('_', "\\_")
        );
        let conn = self.conn();
        let mut st = conn.prepare(
            "SELECT doc_id, MIN(page), text FROM pages WHERE text LIKE ?1 ESCAPE '\\' GROUP BY doc_id",
        )?;
        let rows = st
            .query_map([pattern], |r| {
                let id: String = r.get(0)?;
                let text: String = r.get(2)?;
                Ok((DocumentId(uuid(&id).map_err(|e| conv(0, e))?), text))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// Remove a document and everything derived from it (pages, detections,
    /// questions, sealed source, output link). The file is not touched; the
    /// vault keeps its tokens, which other documents may share.
    pub fn delete_document(&self, id: DocumentId) -> Result<()> {
        let n = self
            .conn()
            .execute("DELETE FROM documents WHERE id=?1", [id.0.to_string()])?;
        if n == 0 {
            return Err(Error::UnknownHandle(id.0.to_string()));
        }
        Ok(())
    }

    /// Remove a folder record. Its children must be deleted first.
    pub fn delete_folder(&self, id: FolderId) -> Result<()> {
        let conn = self.conn();
        let children: i64 = conn.query_row(
            "SELECT COUNT(*) FROM documents WHERE folder_id=?1",
            [id.0.to_string()],
            |r| r.get(0),
        )?;
        if children > 0 {
            return Err(Error::Index("folder still has documents".into()));
        }
        let n = conn.execute("DELETE FROM folders WHERE id=?1", [id.0.to_string()])?;
        if n == 0 {
            return Err(Error::UnknownHandle(id.0.to_string()));
        }
        Ok(())
    }

    /// Mark `output` as written by an agent from the document or folder `source`.
    pub fn record_output(&self, output: DocumentId, source: &Codename) -> Result<()> {
        self.conn().execute(
            "INSERT OR REPLACE INTO outputs (doc_id, source) VALUES (?1,?2)",
            params![output.0.to_string(), source.as_str()],
        )?;
        Ok(())
    }

    /// The source codename of an agent-written output, if it is one.
    pub fn output_source(&self, output: DocumentId) -> Result<Option<Codename>> {
        let s: Option<String> = self
            .conn()
            .query_row(
                "SELECT source FROM outputs WHERE doc_id=?1",
                [output.0.to_string()],
                |r| r.get(0),
            )
            .optional()?;
        s.map(|s| codename(&s)).transpose()
    }
}

fn check_transition(from: DocState, to: DocState) -> Result<()> {
    if from.can_transition_to(to) {
        Ok(())
    } else {
        Err(Error::Index(format!(
            "illegal state change {from:?} -> {to:?}"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DetectionSource, IngestMode, Token, TokenKind, Treatment};
    use chrono::Utc;

    fn doc(code: &str, folder: Option<FolderId>) -> DocumentRecord {
        DocumentRecord {
            id: DocumentId::new(),
            codename: Codename::parse(code).unwrap(),
            path: Utf8PathBuf::from(format!("/p/{code}.pdf")),
            original_name: format!("{code}.pdf"),
            kind: DocKind("w2".into()),
            year: Some(2025),
            state: DocState::Queued,
            inked: 0,
            folder,
            ingest: IngestMode::Move,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            status_note: None,
            tag: None,
            payer_token: None,
        }
    }

    fn det(needs_review: bool) -> Detection {
        Detection {
            page: 1,
            span: Span { start: 0, end: 3 },
            kind: TokenKind::Account,
            source: DetectionSource::Model { name: "m".into() },
            confidence: 0.4,
            treatment: Treatment::Tokenize,
            needs_review,
            review_prompt: Some("Looks like an account number".into()),
            role: None,
            generalized: None,
            field: None,
        }
    }

    #[test]
    fn opens_file_with_wal_and_migrates_once() {
        let d = tempfile::tempdir().unwrap();
        let p = Utf8PathBuf::from_path_buf(d.path().join("index.db")).unwrap();
        let ix = Index::open(&p).unwrap();
        assert_eq!(ix.schema_version().unwrap(), SCHEMA_VERSION);
        assert_eq!(ix.journal_mode().unwrap(), "wal");
        let r = doc("w2-acme-2025", None);
        ix.insert_document(&r, false).unwrap();
        drop(ix);
        let ix = Index::open(&p).unwrap();
        assert_eq!(ix.schema_version().unwrap(), SCHEMA_VERSION);
        assert_eq!(ix.document(r.id).unwrap().unwrap(), r_roundtrip(&r));
    }

    #[test]
    fn upgrades_a_version_1_index() {
        let d = tempfile::tempdir().unwrap();
        let p = Utf8PathBuf::from_path_buf(d.path().join("index.db")).unwrap();
        {
            let conn = Connection::open(&p).unwrap();
            conn.execute_batch(
                "CREATE TABLE schema_version (version INTEGER NOT NULL);\
                 INSERT INTO schema_version (version) VALUES (1);",
            )
            .unwrap();
            conn.execute_batch(MIGRATIONS[0]).unwrap();
        }
        let ix = Index::open(&p).unwrap();
        assert_eq!(ix.schema_version().unwrap(), SCHEMA_VERSION);
        let r = doc("a", None);
        ix.insert_document(&r, false).unwrap();
        let q = ix
            .add_question(r.id, 0, 1, &TokenKind::Account, None)
            .unwrap();
        assert_eq!(ix.question(q).unwrap().unwrap().value_length, 0);
        ix.set_question_peek(q, 9, 1, None, b"x").unwrap();
        assert_eq!(ix.question(q).unwrap().unwrap().value_length, 9);
    }

    // Timestamps round-trip at microsecond precision.
    fn r_roundtrip(r: &DocumentRecord) -> DocumentRecord {
        let mut r = r.clone();
        r.created_at = parse_ts(&ts(r.created_at)).unwrap();
        r.updated_at = parse_ts(&ts(r.updated_at)).unwrap();
        r
    }

    #[test]
    fn insert_find_list_update() {
        let ix = Index::open_in_memory().unwrap();
        let mut r = doc("w2-acme-2025", None);
        ix.insert_document(&r, true).unwrap();
        assert!(ix.is_provisional(r.id).unwrap());
        assert!(ix.insert_document(&r, false).is_err(), "duplicate id");
        let by_code = ix.find_by_codename(&r.codename).unwrap().unwrap();
        assert_eq!(by_code.id, r.id);
        assert_eq!(ix.find_by_path(&r.path).unwrap().unwrap().id, r.id);
        assert!(
            ix.find_by_codename(&Codename::parse("nope").unwrap())
                .unwrap()
                .is_none()
        );
        assert!(ix.codename_taken("w2-acme-2025").unwrap());
        assert!(!ix.codename_taken("w2-acme-2025-2").unwrap());

        r.inked = 6;
        r.status_note = Some("page 3 of 11".into());
        ix.update_document(&r).unwrap();
        assert_eq!(ix.document(r.id).unwrap().unwrap().inked, 6);
        // update_document enforces the state machine too.
        r.state = DocState::Ready;
        assert!(matches!(ix.update_document(&r), Err(Error::Index(_))));

        ix.set_codename(r.id, &Codename::parse("w2-other-2025").unwrap())
            .unwrap();
        assert!(!ix.is_provisional(r.id).unwrap());
        assert_eq!(ix.list_documents().unwrap().len(), 1);
        assert_eq!(
            ix.list_documents().unwrap()[0].codename.as_str(),
            "w2-other-2025"
        );
    }

    #[test]
    fn state_machine_enforced() {
        let ix = Index::open_in_memory().unwrap();
        let r = doc("a", None);
        ix.insert_document(&r, false).unwrap();
        assert!(ix.set_state(r.id, DocState::Ready, None).is_err());
        ix.set_state(r.id, DocState::Reading, Some("reading the scan"))
            .unwrap();
        ix.set_state(r.id, DocState::Inking, None).unwrap();
        ix.set_state(r.id, DocState::NeedsAttention, None).unwrap();
        assert!(ix.set_state(r.id, DocState::Ready, None).is_err());
        ix.set_state(r.id, DocState::Inking, None).unwrap();
        let after = ix.set_state(r.id, DocState::Ready, None).unwrap();
        assert_eq!(after.state, DocState::Ready);
        assert_eq!(after.status_note, None);
        assert!(matches!(
            ix.set_state(DocumentId::new(), DocState::Reading, None),
            Err(Error::UnknownHandle(_))
        ));
    }

    #[test]
    fn folders_children_and_summary() {
        let ix = Index::open_in_memory().unwrap();
        let f = FolderRecord {
            id: FolderId::new(),
            codename: Codename::parse("taxes-2025").unwrap(),
            path: "/p/Taxes 2025".into(),
            created_at: Utc::now(),
        };
        ix.insert_folder(&f).unwrap();
        assert_eq!(ix.folder(f.id).unwrap().unwrap().codename, f.codename);
        assert_eq!(
            ix.find_folder_by_codename(&f.codename).unwrap().unwrap().id,
            f.id
        );
        assert_eq!(ix.find_folder_by_path(&f.path).unwrap().unwrap().id, f.id);
        assert_eq!(ix.list_folders().unwrap().len(), 1);
        assert!(ix.codename_taken("taxes-2025").unwrap());

        let a = doc("a", Some(f.id));
        let b = doc("b", Some(f.id));
        let c = doc("c", Some(f.id));
        let other = doc("d", None);
        for r in [&a, &b, &c, &other] {
            ix.insert_document(r, false).unwrap();
        }
        for s in [DocState::Reading, DocState::Inking, DocState::Ready] {
            ix.set_state(a.id, s, None).unwrap();
        }
        let mut a2 = ix.document(a.id).unwrap().unwrap();
        a2.inked = 4;
        ix.update_document(&a2).unwrap();
        for s in [
            DocState::Reading,
            DocState::Inking,
            DocState::NeedsAttention,
        ] {
            ix.set_state(b.id, s, None).unwrap();
        }
        assert_eq!(ix.folder_children(f.id).unwrap().len(), 3);
        let s = ix.folder_summary(f.id).unwrap();
        assert_eq!(
            s,
            FolderSummary {
                total: 3,
                ready: 1,
                inking: 1,
                needs_attention: 1,
                failed: 0,
                inked: 4
            }
        );
    }

    #[test]
    fn pages_derived_detections_sealed() {
        let ix = Index::open_in_memory().unwrap();
        let r = doc("a", None);
        ix.insert_document(&r, false).unwrap();
        let pages = vec![
            ResolvedPage {
                page: 1,
                text: "one [SSN_1]".into(),
            },
            ResolvedPage {
                page: 2,
                text: "two".into(),
            },
        ];
        ix.store_pages(r.id, &pages).unwrap();
        ix.store_pages(r.id, &pages).unwrap(); // replace, not append
        assert_eq!(ix.pages(r.id).unwrap(), pages);

        let derived = DerivedData {
            fields: vec![FormField {
                name: "f".into(),
                value: "[SSN_1]".into(),
                page: Some(1),
            }],
            roles: vec![TokenRole {
                token: Token {
                    kind: TokenKind::Ssn,
                    index: 1,
                },
                role: "employee".into(),
            }],
            treatments: vec![KindTreatment {
                kind: TokenKind::Ssn,
                treatment: Treatment::Tokenize,
                count: 1,
            }],
        };
        assert!(ix.derived(r.id).unwrap().is_none());
        ix.store_derived(r.id, &derived).unwrap();
        assert_eq!(ix.derived(r.id).unwrap().unwrap(), derived);

        let dets = vec![det(false), det(true)];
        ix.store_detections(r.id, &dets).unwrap();
        assert_eq!(ix.detections(r.id).unwrap(), dets);

        assert!(ix.sealed_source(r.id).unwrap().is_none());
        ix.store_sealed_source(r.id, b"cipher").unwrap();
        assert_eq!(ix.sealed_source(r.id).unwrap().unwrap(), b"cipher");
        ix.delete_sealed_source(r.id).unwrap();
        assert!(ix.sealed_source(r.id).unwrap().is_none());
    }

    #[test]
    fn review_questions() {
        let ix = Index::open_in_memory().unwrap();
        let r = doc("a", None);
        ix.insert_document(&r, false).unwrap();
        let q1 = ix
            .add_question(
                r.id,
                1,
                1,
                &TokenKind::Account,
                Some("Looks like an account"),
            )
            .unwrap();
        let q2 = ix
            .add_question(r.id, 3, 2, &TokenKind::Person, None)
            .unwrap();
        assert_eq!(ix.questions(Some(r.id), false).unwrap().len(), 2);
        assert_eq!(ix.questions(None, false).unwrap().len(), 2);
        ix.set_question_peek(q1, 11, 2, Some(Span { start: 40, end: 51 }), b"sealed")
            .unwrap();
        let q = ix.question(q1).unwrap().unwrap();
        assert_eq!((q.value_length, q.page_count), (11, 2));
        // Where the value sits in its page's text, for the card's page crop.
        assert_eq!(q.span, Some(Span { start: 40, end: 51 }));
        assert_eq!(
            ix.question(q2).unwrap().unwrap().span,
            None,
            "never recorded, or a form-field value with no place on the page"
        );
        // The index only ever holds the context sealed.
        assert!(q.context_before.is_empty() && q.context_after.is_empty());
        assert_eq!(
            ix.question_context(q1).unwrap().as_deref(),
            Some(&b"sealed"[..])
        );
        assert_eq!(ix.question_context(q2).unwrap(), None);
        let ans = ix.answer_question(q1, ReviewAnswer::InkIt).unwrap();
        assert_eq!(ans.answer, Some(ReviewAnswer::InkIt));
        // Answered: the card is gone, so is its context.
        assert_eq!(ix.question_context(q1).unwrap(), None);
        assert_eq!(ix.question(q1).unwrap().unwrap().value_length, 11);
        assert!(ans.answered_at.is_some());
        assert!(matches!(
            ix.answer_question(q1, ReviewAnswer::LeaveIt),
            Err(Error::Index(_))
        ));
        assert!(matches!(
            ix.answer_question(QuestionId(999), ReviewAnswer::LeaveIt),
            Err(Error::Index(_))
        ));
        assert_eq!(ix.questions(Some(r.id), false).unwrap().len(), 1);
        assert_eq!(ix.questions(Some(r.id), true).unwrap().len(), 2);
        ix.answer_question(q2, ReviewAnswer::LeaveIt).unwrap();
        assert_eq!(ix.question(q2).unwrap().unwrap().kind, TokenKind::Person);
        assert_eq!(
            ix.answered_detections(r.id).unwrap(),
            vec![(1, ReviewAnswer::InkIt), (3, ReviewAnswer::LeaveIt)]
        );
        ix.clear_questions(r.id).unwrap();
        assert!(ix.questions(Some(r.id), true).unwrap().is_empty());
    }

    #[test]
    fn agent_reads_and_outputs() {
        let ix = Index::open_in_memory().unwrap();
        let code = Codename::parse("w2-acme-2025").unwrap();
        ix.record_agent_read("Claude Code", &code).unwrap();
        ix.record_agent_read("Codex", &code).unwrap();
        let reads = ix.recent_reads(1).unwrap();
        assert_eq!(reads.len(), 1);
        assert_eq!(reads[0].agent, "Codex");
        assert_eq!(reads[0].handle.to_string(), "🐙/w2-acme-2025");
        assert_eq!(ix.recent_reads(10).unwrap().len(), 2);

        let src = doc("a", None);
        let out = doc("a-summary", None);
        ix.insert_document(&src, false).unwrap();
        ix.insert_document(&out, false).unwrap();
        assert_eq!(ix.output_source(out.id).unwrap(), None);
        ix.record_output(out.id, &src.codename).unwrap();
        assert_eq!(
            ix.output_source(out.id).unwrap(),
            Some(src.codename.clone())
        );
    }

    #[test]
    fn debug_is_safe() {
        let ix = Index::open_in_memory().unwrap();
        assert!(format!("{ix:?}").starts_with("Index"));
    }

    #[test]
    fn tag_and_payer_token_round_trip_and_default_to_none() {
        let ix = Index::open_in_memory().unwrap();
        let mut r = doc("w2-2025", None);
        r.tag = Some("Taxes".into());
        r.payer_token = Some(Token {
            kind: TokenKind::Org,
            index: 3,
        });
        ix.insert_document(&r, false).unwrap();
        let back = ix.document(r.id).unwrap().unwrap();
        assert_eq!(back.tag.as_deref(), Some("Taxes"));
        assert_eq!(
            back.payer_token
                .as_ref()
                .map(ToString::to_string)
                .as_deref(),
            Some("[ORG_3]")
        );
        let mut r2 = back.clone();
        r2.tag = None;
        r2.payer_token = None;
        ix.update_document(&r2).unwrap();
        let back = ix.document(r.id).unwrap().unwrap();
        assert_eq!(back.tag, None);
        assert_eq!(back.payer_token, None);
        assert_eq!(parse_token("[ORG_3]").map(|t| t.index), Some(3));
        assert_eq!(parse_token("x [ORG_3]"), None);
        assert_eq!(parse_token("ORG_3"), None);
        assert_eq!(
            parse_token("[CONTROL_NUMBER_2]"),
            Some(Token {
                kind: TokenKind::Custom("CONTROL_NUMBER".into()),
                index: 2
            })
        );
    }

    #[test]
    fn last_read_is_per_codename_and_newest() {
        let ix = Index::open_in_memory().unwrap();
        let a = Codename::parse("w2-2025").unwrap();
        let b = Codename::parse("w2-2025-2").unwrap();
        assert!(ix.last_read(&a).unwrap().is_none());
        ix.record_agent_read("Claude", &a).unwrap();
        ix.record_agent_read("Claude Code", &b).unwrap();
        ix.record_agent_read("ChatGPT", &a).unwrap();
        assert_eq!(ix.last_read(&a).unwrap().unwrap().agent, "ChatGPT");
        assert_eq!(ix.last_read(&b).unwrap().unwrap().agent, "Claude Code");
    }

    #[test]
    fn search_pages_finds_tokenized_text_and_escapes_like_wildcards() {
        let ix = Index::open_in_memory().unwrap();
        let a = doc("1098-2025", None);
        let b = doc("w2-2025", None);
        ix.insert_document(&a, false).unwrap();
        ix.insert_document(&b, false).unwrap();
        let page = |n, t: &str| ResolvedPage {
            page: n,
            text: t.into(),
        };
        ix.store_pages(
            a.id,
            &[
                page(1, "Form 1098"),
                page(2, "Mortgage Interest Statement [PERSON_1]"),
            ],
        )
        .unwrap();
        ix.store_pages(b.id, &[page(1, "Wage and Tax Statement 100% [SSN_1]")])
            .unwrap();
        let hits = ix.search_pages("mortgage interest").unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].0, a.id);
        assert!(hits[0].1.contains("Mortgage Interest"));
        assert_eq!(ix.search_pages("100%").unwrap().len(), 1);
        assert_eq!(
            ix.search_pages("10_").unwrap().len(),
            0,
            "underscore is literal"
        );
        assert_eq!(ix.search_pages("zzz").unwrap().len(), 0);
    }

    #[test]
    fn delete_document_cascades_and_delete_folder_needs_an_empty_folder() {
        let ix = Index::open_in_memory().unwrap();
        let f = FolderRecord {
            id: FolderId::new(),
            codename: Codename::parse("folder-0412").unwrap(),
            path: Utf8PathBuf::from("/p/Taxes 2025"),
            created_at: Utc::now(),
        };
        ix.insert_folder(&f).unwrap();
        let r = doc("w2-2025", Some(f.id));
        ix.insert_document(&r, false).unwrap();
        ix.store_pages(
            r.id,
            &[ResolvedPage {
                page: 1,
                text: "[SSN_1]".into(),
            }],
        )
        .unwrap();
        ix.add_question(r.id, 0, 1, &TokenKind::Account, None)
            .unwrap();
        assert!(ix.delete_folder(f.id).is_err(), "still has a child");
        ix.delete_document(r.id).unwrap();
        assert!(ix.document(r.id).unwrap().is_none());
        assert!(ix.pages(r.id).unwrap().is_empty());
        assert!(ix.questions(Some(r.id), true).unwrap().is_empty());
        assert!(ix.delete_document(r.id).is_err(), "gone");
        ix.delete_folder(f.id).unwrap();
        assert!(ix.folder(f.id).unwrap().is_none());
    }
}

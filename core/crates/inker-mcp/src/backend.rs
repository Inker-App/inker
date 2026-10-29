//! The seam between the MCP server and the core.
//!
//! `inker-core`'s `Core` facade is being built in parallel; this trait is what
//! the server needs from it. At integration time `Core` implements `Backend`
//! (or a thin adapter does), and `Resolved`/`WriteReceipt` move to the core if
//! it grows equivalents.

use camino::Utf8PathBuf;
use inker_core::{
    Description, DocState, Handle, ListEntry, QuestionId, ResolvedDocument, ResolvedFolder,
    ReviewAnswer, TokenKind,
};
use serde::{Deserialize, Serialize};

/// What `resolve` returns from the core.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum Resolved {
    Document(ResolvedDocument),
    Folder(ResolvedFolder),
}

/// What `write` returns from the core. The path is inside the private folder
/// and is never sent to the agent; only the handle is.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WriteReceipt {
    pub path: Utf8PathBuf,
    pub handle: Handle,
}

/// One open attention-card question, as the CLI shows it. Never holds the
/// value: only where it is, its kind, the prompt and the words around it
/// (digit runs already blanked by the core).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PendingReview {
    pub id: QuestionId,
    pub handle: Handle,
    pub kind: TokenKind,
    pub page: u32,
    /// Pages in the document; 0 when unknown.
    pub page_count: u32,
    pub prompt: Option<String>,
    pub context_before: String,
    pub context_after: String,
}

/// What the running server is serving, for `inker doctor`. Holds paths, so
/// it is served only to the INKER CLI, never to an agent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerStatus {
    pub private_folder: Utf8PathBuf,
}

/// Everything the MCP server needs from the core.
///
/// Implementations must return tokenized content only. Errors map to agent
/// messages: `PendingReview` and `NotReady` become the fixed sentences in
/// [`crate::server`]; anything else becomes a generic error with no detail.
pub trait Backend: Send + Sync {
    fn list(&self) -> inker_core::Result<Vec<ListEntry>>;
    fn resolve(&self, h: &Handle) -> inker_core::Result<Resolved>;
    fn describe(&self, h: &Handle) -> inker_core::Result<Description>;
    /// `content` is tokenized text; the core rehydrates and saves it.
    fn write(&self, h: &Handle, name: &str, content: &str) -> inker_core::Result<WriteReceipt>;
    /// Record that `agent` read `h`, for the Ready board's last-read line.
    fn note_read(&self, agent: &str, h: &Handle);
    /// Open review questions, oldest first. CLI only (`inker/review`).
    fn pending_reviews(&self) -> inker_core::Result<Vec<PendingReview>>;
    /// Answer one question; returns its document's state afterwards (Ready
    /// once its last question is answered). CLI only (`inker/answer`).
    fn answer_review(&self, id: QuestionId, answer: ReviewAnswer) -> inker_core::Result<DocState>;
    /// What this server serves. CLI only (`inker/status`).
    fn status(&self) -> inker_core::Result<ServerStatus>;
}

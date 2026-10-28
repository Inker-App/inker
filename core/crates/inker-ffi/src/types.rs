//! Records and enums that cross the FFI.
//!
//! These mirror the shared types in `inker_core::types` rather than deriving
//! UniFFI traits there, so `inker-core` stays free of FFI concerns. `From`
//! impls convert at the boundary.

use std::time::SystemTime;

/// Whether a drop moved or copied the original. Mirrors `inker_core::IngestMode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum IngestMode {
    Move,
    Copy,
}

impl From<IngestMode> for inker_core::types::IngestMode {
    fn from(m: IngestMode) -> Self {
        match m {
            IngestMode::Move => Self::Move,
            IngestMode::Copy => Self::Copy,
        }
    }
}

impl From<inker_core::types::IngestMode> for IngestMode {
    fn from(m: inker_core::types::IngestMode) -> Self {
        match m {
            inker_core::types::IngestMode::Move => Self::Move,
            inker_core::types::IngestMode::Copy => Self::Copy,
        }
    }
}

/// How a handle is rendered for the clipboard. Mirrors `inker_core::HandleStyle`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum HandleStyle {
    /// `🐙/w2-acme-2025`
    Octopus,
    /// `ink/w2-acme-2025`
    Ascii,
}

impl From<HandleStyle> for inker_core::types::HandleStyle {
    fn from(s: HandleStyle) -> Self {
        match s {
            HandleStyle::Octopus => Self::Octopus,
            HandleStyle::Ascii => Self::Ascii,
        }
    }
}

/// The user's answer on the attention card. Mirrors `inker_core::ReviewAnswer`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum ReviewAnswer {
    /// "Ink it"
    InkIt,
    /// "Leave it" (this document only, decision 5.9)
    LeaveIt,
}

impl From<ReviewAnswer> for inker_core::types::ReviewAnswer {
    fn from(a: ReviewAnswer) -> Self {
        match a {
            ReviewAnswer::InkIt => Self::InkIt,
            ReviewAnswer::LeaveIt => Self::LeaveIt,
        }
    }
}

impl From<inker_core::types::ReviewAnswer> for ReviewAnswer {
    fn from(a: inker_core::types::ReviewAnswer) -> Self {
        match a {
            inker_core::types::ReviewAnswer::InkIt => Self::InkIt,
            inker_core::types::ReviewAnswer::LeaveIt => Self::LeaveIt,
        }
    }
}

/// Queue state of one document. Mirrors `inker_core::DocState`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum DocState {
    Queued,
    Reading,
    Inking,
    NeedsAttention,
    Ready,
    Failed,
}

impl From<inker_core::types::DocState> for DocState {
    fn from(s: inker_core::types::DocState) -> Self {
        use inker_core::types::DocState as S;
        match s {
            S::Queued => Self::Queued,
            S::Reading => Self::Reading,
            S::Inking => Self::Inking,
            S::NeedsAttention => Self::NeedsAttention,
            S::Ready => Self::Ready,
            S::Failed => Self::Failed,
        }
    }
}

impl DocState {
    /// Still being worked on (counts toward "3 inking").
    pub fn in_progress(self) -> bool {
        matches!(
            self,
            DocState::Queued | DocState::Reading | DocState::Inking
        )
    }
}

/// Which ink-grid loader the queue row shows (`AppPopProcessing`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum Loader {
    /// No loader: the row is done, held or failed. (Not `None`: that would
    /// collide with `Optional.none` in Swift.)
    Idle,
    /// The travelling pulse, for text-layer reading and inking.
    Read,
    /// The scanning head, for OCR ("reading the scan").
    Ocr,
    /// Flat, faint dots.
    Queued,
}

/// A held value, as the attention card shows it. Never contains the value.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ReviewQuestion {
    /// Pass back to `answer`.
    pub id: String,
    /// "Looks like an account number, but the scan is rough. Ink it?"
    pub prompt: String,
    /// "page 3 of 11"
    pub location: String,
    /// Words just before the value, already safe ("…wire to"). May be empty.
    pub context_before: String,
    /// How many characters the dotted outline stands in for. The value itself
    /// never leaves the core.
    pub value_length: u32,
}

/// One document row.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct DocItem {
    /// Stable id (the core's `DocumentId`), as a string.
    pub id: String,
    /// `w2-acme-2025`; the shell adds the `🐙/` prefix via `handle_string`.
    pub codename: String,
    /// File name at drop time, shown while processing.
    pub original_name: String,
    /// Where the original lives now, for "Open file" (reveal in Finder).
    pub path: String,
    /// Human kind label: "W-2", "1099-INT", "scan", "closing statement".
    pub kind_label: String,
    pub state: DocState,
    /// Values inked so far, for "6 inked".
    pub inked: u32,
    /// "reading", "reading the scan", "queued", "page 3 of 11".
    pub status_note: Option<String>,
    pub loader: Loader,
    /// Set while `state == NeedsAttention`.
    pub question: Option<ReviewQuestion>,
}

/// "4 files · 1 ready · 3 inking". Mirrors `inker_core::FolderSummary`.
#[derive(Debug, Clone, Default, PartialEq, Eq, uniffi::Record)]
pub struct FolderSummary {
    pub total: u32,
    pub ready: u32,
    pub inking: u32,
    pub needs_attention: u32,
    pub failed: u32,
    pub inked: u32,
}

impl From<inker_core::types::FolderSummary> for FolderSummary {
    fn from(s: inker_core::types::FolderSummary) -> Self {
        Self {
            total: s.total,
            ready: s.ready,
            inking: s.inking,
            needs_attention: s.needs_attention,
            failed: s.failed,
            inked: s.inked,
        }
    }
}

impl FolderSummary {
    pub fn of(children: &[DocItem]) -> Self {
        let mut s = Self::default();
        for c in children {
            s.total += 1;
            s.inked += c.inked;
            match c.state {
                DocState::Ready => s.ready += 1,
                DocState::NeedsAttention => s.needs_attention += 1,
                DocState::Failed => s.failed += 1,
                DocState::Queued | DocState::Reading | DocState::Inking => s.inking += 1,
            }
        }
        s
    }
}

/// One entry in the popover queue: a dropped folder with its children, or a
/// single dropped file (then `is_folder` is false and `children` holds exactly
/// that file).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct QueueItem {
    pub id: String,
    pub codename: String,
    pub is_folder: bool,
    pub summary: FolderSummary,
    pub children: Vec<DocItem>,
}

/// "Claude Code read 🐙/w2-acme-2025, saw only ink · 2m".
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ReadEvent {
    /// Client name as the MCP client reported it.
    pub agent: String,
    pub codename: String,
    pub at: SystemTime,
}

/// What the footer needs.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct Status {
    /// "MCP running" means the socket is listening (decision 5.14).
    pub mcp_listening: bool,
    /// The private folder as the user should read it, e.g. `~/Private`.
    pub private_folder_display: String,
}

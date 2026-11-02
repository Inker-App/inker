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
    /// The page the value is on, counted from 0, for the card's page crop.
    pub page_index: u32,
    /// Words just before the value, already safe ("…wire to"). May be empty.
    pub context_before: String,
    /// How many characters the dotted outline stands in for. The value itself
    /// never leaves the core.
    pub value_length: u32,
    /// Where the value sits in that page's text, so the shell can re-read the
    /// file and show the page around it. `None` for a form field's value.
    pub span: Option<TextSpan>,
    /// Whether "Leave it" may be offered. `false` for an SSN-shaped value:
    /// the core refuses that answer, so the card shows "Ink it" alone.
    pub can_leave: bool,
}

/// A byte range into a page's text, as the core indexed it. On macOS that
/// text came from PDFKit or Vision through the shell's own providers, so the
/// shell can map it back onto the page.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct TextSpan {
    pub start: u32,
    pub end: u32,
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
    /// Tax year, once classified; the row shows it after the form type.
    pub year: Option<u16>,
    /// "Acme Robotics": the employer, payer or lender a template attributed.
    /// Shown in the popover only; a setting hides it.
    pub payer: Option<String>,
    /// The template's tag ("Taxes"); Find matches it. The board does not
    /// group by it.
    pub tag: Option<String>,
    /// When it was dropped.
    pub dropped_at: SystemTime,
    /// Original name of the folder it came in with, if any.
    pub folder_name: Option<String>,
}

/// A folder on the board, as it is on disk. The board's root is the private
/// folder itself. UniFFI carries the recursion: a Swift struct may hold an
/// array of its own type.
#[derive(Debug, Clone, Default, PartialEq, Eq, uniffi::Record)]
pub struct FolderItem {
    /// The dropped folder's id, or `dir:` and the relative path for a
    /// folder that is only a path segment (and for the root, `dir:`).
    pub id: String,
    /// `folder-0412` for a dropped folder; `None` for a folder that is only
    /// a path segment, which has no handle.
    pub codename: Option<String>,
    /// The folder's name on disk, for line 1 of its row; empty for the root.
    pub name: String,
    /// Relative to the private folder (`Taxes 2025/Receipts`), empty for
    /// the root. The shell remembers open folders by it.
    pub rel_path: String,
    /// Where it lives now, for Show in Finder.
    pub path: String,
    /// Everything under it, at any depth; `total` is the file count.
    pub summary: FolderSummary,
    /// Sub-folders, in Finder order (case-insensitive, numbers as numbers).
    pub folders: Vec<FolderItem>,
    /// Files directly inside, after the sub-folders, in Finder order.
    pub files: Vec<DocItem>,
}

impl FolderItem {
    /// Convert a tree laid out by `inker_core::find::build_tree`. `root` is
    /// the private folder's path, `doc` converts a file and `dropped` gives
    /// a dropped folder's id and codename.
    pub fn from_tree<D, F>(
        t: inker_core::find::Tree<D, F>,
        root: &str,
        doc: &impl Fn(D) -> DocItem,
        dropped: &impl Fn(&F) -> (String, String),
    ) -> Self {
        let folders: Vec<FolderItem> = t
            .folders
            .into_iter()
            .map(|f| FolderItem::from_tree(f, root, doc, dropped))
            .collect();
        let files: Vec<DocItem> = t.files.into_iter().map(doc).collect();
        let mut summary = FolderSummary::of(&files);
        for f in &folders {
            summary.add(&f.summary);
        }
        let (id, codename) = match t.folder.as_ref().map(dropped) {
            Some((id, code)) => (id, Some(code)),
            None => (format!("dir:{}", t.path), None),
        };
        let path = if t.path.is_empty() {
            root.to_string()
        } else {
            format!("{}/{}", root.trim_end_matches('/'), t.path)
        };
        FolderItem {
            id,
            codename,
            name: t.name,
            rel_path: t.path,
            path,
            summary,
            folders,
            files,
        }
    }

    /// The document or dropped folder with this codename, at any depth.
    pub fn locate(&self, codename: &str) -> Option<BoardEntry> {
        if self.codename.as_deref() == Some(codename) {
            return Some(BoardEntry::Folder { item: self.clone() });
        }
        if let Some(d) = self.files.iter().find(|d| d.codename == codename) {
            return Some(BoardEntry::Doc { item: d.clone() });
        }
        self.folders.iter().find_map(|f| f.locate(codename))
    }

    /// Every file under this folder, depth first.
    pub fn all_files(&self) -> Vec<&DocItem> {
        let mut out: Vec<&DocItem> = Vec::new();
        for f in &self.folders {
            out.extend(f.all_files());
        }
        out.extend(self.files.iter());
        out
    }
}

/// What `locate` found: a document or a dropped folder. A UniFFI enum
/// cannot box its payload, so the size difference is accepted.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BoardEntry {
    Doc { item: DocItem },
    Folder { item: FolderItem },
}

impl BoardEntry {
    pub fn id(&self) -> &str {
        match self {
            BoardEntry::Doc { item } => &item.id,
            BoardEntry::Folder { item } => &item.id,
        }
    }

    /// The handle's codename; `None` only for a folder that has no handle.
    pub fn codename(&self) -> Option<&str> {
        match self {
            BoardEntry::Doc { item } => Some(&item.codename),
            BoardEntry::Folder { item } => item.codename.as_deref(),
        }
    }
}

/// The ready board: the private folder as it is on disk, every row under
/// its original name with its handle. Mirrors `inker_core::find::Board`.
#[derive(Debug, Clone, PartialEq, Eq, Default, uniffi::Record)]
pub struct Board {
    /// The private folder itself: its sub-folders, then its files.
    pub root: FolderItem,
    /// Every document, whatever its state.
    pub total: u32,
}

impl Board {
    /// The document or dropped folder with this codename, at any depth.
    pub fn locate(&self, codename: &str) -> Option<BoardEntry> {
        self.root.locate(codename)
    }
}

/// Which field a Find query matched, in ranking order. Mirrors
/// `inker_core::find::MatchField`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, uniffi::Enum)]
pub enum MatchField {
    FileName,
    Handle,
    Kind,
    Payer,
    Year,
    Folder,
    Tag,
    Text,
}

impl From<inker_core::find::MatchField> for MatchField {
    fn from(f: inker_core::find::MatchField) -> Self {
        use inker_core::find::MatchField as F;
        match f {
            F::FileName => Self::FileName,
            F::Handle => Self::Handle,
            F::Kind => Self::Kind,
            F::Payer => Self::Payer,
            F::Year => Self::Year,
            F::Folder => Self::Folder,
            F::Tag => Self::Tag,
            F::Text => Self::Text,
        }
    }
}

/// One Find result.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct FindMatch {
    pub doc: DocItem,
    pub field: MatchField,
    /// The matched characters as they appear in that field, for underlining.
    pub matched: String,
    /// For a text match: the line around the hit, ellipsised.
    pub snippet: Option<String>,
    /// The query was this document's whole handle.
    pub exact_handle: bool,
}

/// The expanded row.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct DocDetails {
    /// Folder inside the private folder, `Taxes 2025/`, or empty at the root.
    pub location: String,
    pub dropped_at: SystemTime,
    /// Files that came in the same folder drop, not counting this one.
    pub dropped_with: u32,
    pub last_read: Option<ReadEvent>,
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

    /// Fold a sub-folder's counts into this one.
    pub fn add(&mut self, o: &FolderSummary) {
        self.total = self.total.saturating_add(o.total);
        self.ready = self.ready.saturating_add(o.ready);
        self.inking = self.inking.saturating_add(o.inking);
        self.needs_attention = self.needs_attention.saturating_add(o.needs_attention);
        self.failed = self.failed.saturating_add(o.failed);
        self.inked = self.inked.saturating_add(o.inked);
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
    /// Product names of the chat apps and agents with a live MCP session
    /// right now ("Claude", "Claude Code"), each once, sorted. The footer
    /// says "Claude connected" when "Claude" is here.
    pub clients: Vec<String>,
}

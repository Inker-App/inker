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
    /// Tax year, once classified; the board groups by it.
    pub year: Option<u16>,
    /// "Acme Robotics": the employer, payer or lender a template attributed.
    /// Shown in the popover only; a setting hides it.
    pub payer: Option<String>,
    /// The chip it counts under ("Taxes"); `None` is under `All` only.
    pub tag: Option<String>,
    /// When it was dropped.
    pub dropped_at: SystemTime,
    /// Original name of the folder it came in with, if any.
    pub folder_name: Option<String>,
}

/// A dropped folder on the board: a sub-group inside its year.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct FolderItem {
    pub id: String,
    /// `folder-0412`
    pub codename: String,
    /// The folder's original name, for line 1 of its row.
    pub name: String,
    /// Where it lives now, for Show in Finder.
    pub path: String,
    pub summary: FolderSummary,
    /// Newest drop first.
    pub children: Vec<DocItem>,
}

/// One row of a year group. A UniFFI enum cannot box its payload, so the
/// size difference between the variants is accepted.
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

    pub fn codename(&self) -> &str {
        match self {
            BoardEntry::Doc { item } => &item.codename,
            BoardEntry::Folder { item } => &item.codename,
        }
    }

    fn files(&self) -> u32 {
        match self {
            BoardEntry::Doc { .. } => 1,
            BoardEntry::Folder { item } => u32::try_from(item.children.len()).unwrap_or(u32::MAX),
        }
    }

    fn dropped_at(&self) -> Option<SystemTime> {
        match self {
            BoardEntry::Doc { item } => Some(item.dropped_at),
            BoardEntry::Folder { item } => item.children.iter().map(|c| c.dropped_at).max(),
        }
    }

    /// A folder's year is the one most of its files have (ties to the
    /// newest drop among them), as in `inker_core::find`.
    fn year(&self) -> Option<u16> {
        match self {
            BoardEntry::Doc { item } => item.year,
            BoardEntry::Folder { item } => {
                let mut counts: Vec<(u16, usize, SystemTime)> = Vec::new();
                for c in &item.children {
                    let Some(y) = c.year else { continue };
                    match counts.iter_mut().find(|(v, _, _)| *v == y) {
                        Some(e) => {
                            e.1 += 1;
                            e.2 = e.2.max(c.dropped_at);
                        }
                        None => counts.push((y, 1, c.dropped_at)),
                    }
                }
                counts
                    .into_iter()
                    .max_by(|a, b| (a.1, a.2).cmp(&(b.1, b.2)))
                    .map(|(y, _, _)| y)
            }
        }
    }

    fn keep_tag(&self, tag: &str) -> Option<BoardEntry> {
        match self {
            BoardEntry::Doc { item } => (item.tag.as_deref() == Some(tag)).then(|| self.clone()),
            BoardEntry::Folder { item } => {
                let children: Vec<DocItem> = item
                    .children
                    .iter()
                    .filter(|c| c.tag.as_deref() == Some(tag))
                    .cloned()
                    .collect();
                (!children.is_empty()).then(|| BoardEntry::Folder {
                    item: FolderItem {
                        summary: FolderSummary::of(&children),
                        children,
                        ..item.clone()
                    },
                })
            }
        }
    }
}

/// `2025 · 14 FILES`; `year` is `None` for `UNDATED`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct YearGroup {
    pub year: Option<u16>,
    pub files: u32,
    pub entries: Vec<BoardEntry>,
}

/// `Taxes · 24` on a chip.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct TagCount {
    pub tag: String,
    pub count: u32,
}

/// The ready board at any size. Mirrors `inker_core::find::Board`.
#[derive(Debug, Clone, PartialEq, Eq, Default, uniffi::Record)]
pub struct Board {
    /// Newest year first, `UNDATED` last.
    pub groups: Vec<YearGroup>,
    /// Every tag with at least one file, most files first.
    pub tags: Vec<TagCount>,
    /// Every document, whatever its state and whatever chip is active.
    pub total: u32,
}

impl Board {
    /// Group entries by year, newest year first, `UNDATED` last; inside a
    /// year, newest drop first. The preview builds its board with this.
    pub fn assemble(entries: Vec<BoardEntry>, tags: Vec<TagCount>, total: u32) -> Self {
        let mut groups: Vec<YearGroup> = Vec::new();
        for e in entries {
            let year = e.year();
            match groups.iter_mut().find(|g| g.year == year) {
                Some(g) => g.entries.push(e),
                None => groups.push(YearGroup {
                    year,
                    files: 0,
                    entries: vec![e],
                }),
            }
        }
        for g in &mut groups {
            g.entries.sort_by_key(|e| std::cmp::Reverse(e.dropped_at()));
            g.files = g.entries.iter().map(BoardEntry::files).sum();
        }
        groups.sort_by(|a, b| match (a.year, b.year) {
            (Some(x), Some(y)) => y.cmp(&x),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        });
        Self {
            groups,
            tags,
            total,
        }
    }

    /// The board under one chip. Tag counts and the total stay as they are.
    pub fn filtered(self, tag: &str) -> Self {
        let entries: Vec<BoardEntry> = self
            .groups
            .into_iter()
            .flat_map(|g| g.entries)
            .filter_map(|e| e.keep_tag(tag))
            .collect();
        Self::assemble(entries, self.tags, self.total)
    }

    /// Every entry, in board order, folders included.
    pub fn entries(&self) -> impl Iterator<Item = &BoardEntry> {
        self.groups.iter().flat_map(|g| g.entries.iter())
    }

    /// Chip counts from a flat list of documents, most files first.
    pub fn count_tags(docs: &[DocItem]) -> Vec<TagCount> {
        let mut out: Vec<TagCount> = Vec::new();
        for d in docs {
            let Some(t) = d.tag.as_deref() else { continue };
            match out.iter_mut().find(|c| c.tag == t) {
                Some(c) => c.count += 1,
                None => out.push(TagCount {
                    tag: t.to_string(),
                    count: 1,
                }),
            }
        }
        out.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.tag.cmp(&b.tag)));
        out
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

//! Shared domain types. Every other crate depends on these; change them with care.
//!
//! Vocabulary (from the brief): the UI says "ink", not "redact". A value is
//! *inked* when it has been replaced by a token. A document is *held* while it
//! needs attention. A handle is a reference the MCP server resolves; there is
//! no mirror folder.

use std::fmt;

use camino::Utf8PathBuf;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Identifiers and handles
// ---------------------------------------------------------------------------

/// Stable identity of a document across renames and codename changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DocumentId(pub Uuid);

impl DocumentId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for DocumentId {
    fn default() -> Self {
        Self::new()
    }
}

/// Stable identity of a dropped folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct FolderId(pub Uuid);

impl FolderId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for FolderId {
    fn default() -> Self {
        Self::new()
    }
}

/// How a handle is rendered for the clipboard. The server always accepts both.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum HandleStyle {
    /// `🐙/w2-acme-2025` (default).
    #[default]
    Octopus,
    /// `ink/w2-acme-2025` (ASCII fallback, a setting).
    Ascii,
}

pub const HANDLE_PREFIX_OCTOPUS: &str = "🐙/";
pub const HANDLE_PREFIX_ASCII: &str = "ink/";

/// A reference to a document or folder, as pasted into an agent chat.
///
/// Only the codename is stored. The prefix is presentation.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Handle {
    pub codename: Codename,
}

impl Handle {
    pub fn new(codename: Codename) -> Self {
        Self { codename }
    }

    /// Parse `🐙/<codename>` or `ink/<codename>`. Leading and trailing
    /// whitespace is ignored; the codename must be valid.
    pub fn parse(s: &str) -> crate::Result<Self> {
        let s = s.trim();
        let rest = s
            .strip_prefix(HANDLE_PREFIX_OCTOPUS)
            .or_else(|| s.strip_prefix(HANDLE_PREFIX_ASCII))
            .ok_or_else(|| crate::Error::InvalidHandle(s.to_string()))?;
        let codename = Codename::parse(rest).map_err(|_| crate::Error::InvalidHandle(s.to_string()))?;
        Ok(Self { codename })
    }

    /// Render with the given style.
    pub fn render(&self, style: HandleStyle) -> String {
        match style {
            HandleStyle::Octopus => format!("{HANDLE_PREFIX_OCTOPUS}{}", self.codename),
            HandleStyle::Ascii => format!("{HANDLE_PREFIX_ASCII}{}", self.codename),
        }
    }
}

impl fmt::Display for Handle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.render(HandleStyle::Octopus))
    }
}

/// A codename like `w2-acme-2025`: lowercase ASCII letters, digits and single
/// hyphens, 1 to 64 characters, no leading or trailing hyphen.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Codename(String);

impl Codename {
    pub fn parse(s: &str) -> Result<Self, String> {
        let ok_len = (1..=64).contains(&s.len());
        let ok_chars = s
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
        let ok_edges = !s.starts_with('-') && !s.ends_with('-') && !s.contains("--");
        if ok_len && ok_chars && ok_edges {
            Ok(Self(s.to_string()))
        } else {
            Err(format!("invalid codename: {s:?}"))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Codename {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

// ---------------------------------------------------------------------------
// Tokens
// ---------------------------------------------------------------------------

/// What kind of sensitive value a token stands for. The label is what the
/// agent sees inside the brackets, e.g. `SSN` in `[SSN_1]`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TokenKind {
    Ssn,
    Itin,
    Ein,
    /// Bank, brokerage or loan account number.
    Account,
    /// ABA routing number.
    Routing,
    /// Payment card number.
    Card,
    /// A natural person's name.
    Person,
    /// An employer, payer, lender or other organisation.
    Org,
    /// A street address (full). Addresses are usually *generalized*, not tokenized.
    Address,
    Email,
    Phone,
    /// Driver licence, passport, state ID.
    GovId,
    /// Anything a pack defines that is not in this list.
    Custom(String),
}

impl TokenKind {
    /// Bracket label. Custom kinds are upper-cased with non-alphanumerics replaced by `_`.
    pub fn label(&self) -> String {
        match self {
            TokenKind::Ssn => "SSN".into(),
            TokenKind::Itin => "ITIN".into(),
            TokenKind::Ein => "EIN".into(),
            TokenKind::Account => "ACCOUNT".into(),
            TokenKind::Routing => "ROUTING".into(),
            TokenKind::Card => "CARD".into(),
            TokenKind::Person => "PERSON".into(),
            TokenKind::Org => "ORG".into(),
            TokenKind::Address => "ADDRESS".into(),
            TokenKind::Email => "EMAIL".into(),
            TokenKind::Phone => "PHONE".into(),
            TokenKind::GovId => "GOVID".into(),
            TokenKind::Custom(s) => s
                .chars()
                .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_uppercase() } else { '_' })
                .collect(),
        }
    }
}

/// A stable token: the same real value always maps to the same token, across
/// documents and sessions. Rendered as `[SSN_1]`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Token {
    pub kind: TokenKind,
    /// 1-based, global per kind, assigned on first sight and never reused.
    pub index: u32,
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}_{}]", self.kind.label(), self.index)
    }
}

// ---------------------------------------------------------------------------
// Detection
// ---------------------------------------------------------------------------

/// Byte range into a page's text. End is exclusive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn len(&self) -> usize {
        self.end.saturating_sub(self.start)
    }

    pub fn is_empty(&self) -> bool {
        self.end <= self.start
    }
}

/// The three treatments from the brief.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Treatment {
    /// Replace with a stable token.
    Tokenize,
    /// Replace with a coarser value, e.g. a full address becomes "Brooklyn, NY".
    Generalize,
    /// Leave as is (amounts, boxes, dates, state).
    Keep,
}

/// Where a detection came from. Precision order: Template > Rule > Model.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "source")]
pub enum DetectionSource {
    /// A form template matched a labelled box.
    Template { form: String, field: String },
    /// A pattern plus checksum rule.
    Rule { name: String },
    /// An ML pass.
    Model { name: String },
    /// Fail-closed catch-all (e.g. a 9+ digit run near a sensitive label).
    FailClosed { reason: String },
}

/// One sensitive value found in a document, before or after treatment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Detection {
    pub page: u32,
    pub span: Span,
    pub kind: TokenKind,
    pub source: DetectionSource,
    /// 0.0 to 1.0.
    pub confidence: f32,
    pub treatment: Treatment,
    /// True when detection was unsure and the document should be held.
    pub needs_review: bool,
    /// A human sentence for the attention card, e.g. "Looks like an account
    /// number, but the scan is rough." Never contains the value itself.
    pub review_prompt: Option<String>,
    /// Role of this value in the document, for `describe`: "employee",
    /// "employer", "payer", "recipient", "lender", "borrower".
    pub role: Option<String>,
}

/// The user's answer on the attention card.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewAnswer {
    /// "Ink it": treat as sensitive, tokenize.
    InkIt,
    /// "Leave it": keep the value, for this document only.
    LeaveIt,
}

// ---------------------------------------------------------------------------
// Extraction
// ---------------------------------------------------------------------------

/// How the text of a page was obtained.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextOrigin {
    /// The PDF had a text layer.
    TextLayer,
    /// OCR on a scan or photo.
    Ocr,
}

/// Text of one page, with the origin so detection can weight confidence.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PageText {
    /// 1-based.
    pub page: u32,
    pub text: String,
    pub origin: TextOrigin,
}

/// A fillable form field (AcroForm widget) with its value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FormField {
    pub name: String,
    pub value: String,
    pub page: Option<u32>,
}

/// Everything extraction produces for one file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Extracted {
    pub pages: Vec<PageText>,
    pub fields: Vec<FormField>,
}

impl Extracted {
    pub fn full_text(&self) -> String {
        let mut s = String::new();
        for p in &self.pages {
            if !s.is_empty() {
                s.push_str("\n\n");
            }
            s.push_str(&p.text);
        }
        s
    }
}

// ---------------------------------------------------------------------------
// Documents and folders
// ---------------------------------------------------------------------------

/// Document kind, as classified by the active packs. Open-ended on purpose:
/// the tax pack is pluggable and other packs will add kinds.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DocKind(pub String);

impl DocKind {
    pub const UNKNOWN: &'static str = "unknown";
    pub const SCAN: &'static str = "scan";

    pub fn unknown() -> Self {
        Self(Self::UNKNOWN.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The four queue states from the popover, plus the terminal failure state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DocState {
    /// Waiting its turn.
    Queued,
    /// Extracting text ("reading" / "reading the scan").
    Reading,
    /// Running detection ("inking").
    Inking,
    /// Held for the user ("needs you"). Handle not served.
    NeedsAttention,
    /// Done; handle served.
    Ready,
    /// Could not be processed; file was still moved.
    Failed,
}

impl DocState {
    /// Whether an agent asking for this document gets content.
    pub fn is_servable(self) -> bool {
        matches!(self, DocState::Ready)
    }
}

/// Whether the drop moved or copied the original.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IngestMode {
    Move,
    Copy,
}

/// One document in the index. Never contains real sensitive values.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DocumentRecord {
    pub id: DocumentId,
    pub codename: Codename,
    /// Where the original lives now (inside the private folder).
    pub path: Utf8PathBuf,
    /// Original file name at drop time, for the queue row.
    pub original_name: String,
    pub kind: DocKind,
    pub year: Option<u16>,
    pub state: DocState,
    /// Number of values inked, for "6 inked".
    pub inked: u32,
    pub folder: Option<FolderId>,
    pub ingest: IngestMode,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// Short human status for the queue row ("reading the scan", "page 3 of 11").
    pub status_note: Option<String>,
}

/// A dropped folder. Children are documents whose `folder` is this id.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FolderRecord {
    pub id: FolderId,
    pub codename: Codename,
    pub path: Utf8PathBuf,
    pub created_at: DateTime<Utc>,
}

/// Summary of a folder for the queue header: "4 files · 1 ready · 3 inking".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct FolderSummary {
    pub total: u32,
    pub ready: u32,
    pub inking: u32,
    pub needs_attention: u32,
    pub failed: u32,
    pub inked: u32,
}

// ---------------------------------------------------------------------------
// What the MCP server returns
// ---------------------------------------------------------------------------

/// A resolved document: tokenized text only.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResolvedDocument {
    pub handle: Handle,
    pub kind: DocKind,
    pub year: Option<u16>,
    pub pages: Vec<ResolvedPage>,
    /// Form fields with values tokenized.
    pub fields: Vec<FormField>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResolvedPage {
    pub page: u32,
    pub text: String,
}

/// A resolved folder: a listing, not concatenated text (decision 5.5).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResolvedFolder {
    pub handle: Handle,
    pub children: Vec<ListEntry>,
}

/// One row of `list` or a folder listing. Safe to show to an agent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ListEntry {
    pub handle: Handle,
    pub kind: DocKind,
    pub year: Option<u16>,
    pub state: DocState,
    pub inked: u32,
    pub is_folder: bool,
}

/// `describe`: what the tokens in a document stand for, without the values.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Description {
    pub handle: Handle,
    pub kind: DocKind,
    pub year: Option<u16>,
    pub state: DocState,
    /// Token -> role, e.g. `[SSN_1]` -> "employee".
    pub roles: Vec<TokenRole>,
    /// Which treatment each kind received in this document.
    pub treatments: Vec<KindTreatment>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TokenRole {
    pub token: Token,
    pub role: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KindTreatment {
    pub kind: TokenKind,
    pub treatment: Treatment,
    pub count: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handle_parses_both_prefixes() {
        let a = Handle::parse("🐙/w2-acme-2025").unwrap();
        let b = Handle::parse("ink/w2-acme-2025").unwrap();
        assert_eq!(a, b);
        assert_eq!(a.render(HandleStyle::Octopus), "🐙/w2-acme-2025");
        assert_eq!(a.render(HandleStyle::Ascii), "ink/w2-acme-2025");
    }

    #[test]
    fn handle_rejects_bad_input() {
        assert!(Handle::parse("w2-acme-2025").is_err());
        assert!(Handle::parse("🐙/").is_err());
        assert!(Handle::parse("🐙/W2").is_err());
        assert!(Handle::parse("🐙/a--b").is_err());
        assert!(Handle::parse("🐙/-a").is_err());
    }

    #[test]
    fn token_renders_like_the_brief() {
        let t = Token { kind: TokenKind::Ssn, index: 1 };
        assert_eq!(t.to_string(), "[SSN_1]");
        let t = Token { kind: TokenKind::Custom("plan id".into()), index: 3 };
        assert_eq!(t.to_string(), "[PLAN_ID_3]");
    }
}

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

    /// From its string form, as the shell holds it.
    pub fn parse(s: &str) -> Option<Self> {
        Uuid::parse_str(s.trim()).ok().map(Self)
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

    /// From its string form, as the shell holds it.
    pub fn parse(s: &str) -> Option<Self> {
        Uuid::parse_str(s.trim()).ok().map(Self)
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

impl HandleStyle {
    /// `🐙/` or `ink/`.
    pub fn prefix(self) -> &'static str {
        match self {
            HandleStyle::Octopus => HANDLE_PREFIX_OCTOPUS,
            HandleStyle::Ascii => HANDLE_PREFIX_ASCII,
        }
    }
}

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
        let codename =
            Codename::parse(rest).map_err(|_| crate::Error::InvalidHandle(s.to_string()))?;
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
///
/// [`Codename::parse`] is the only constructor; deserialization goes through it too.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Codename(String);

impl TryFrom<String> for Codename {
    type Error = String;
    fn try_from(s: String) -> Result<Self, String> {
        Codename::parse(&s)
    }
}

impl From<Codename> for String {
    fn from(c: Codename) -> String {
        c.0
    }
}

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
                .map(|c| {
                    if c.is_ascii_alphanumeric() {
                        c.to_ascii_uppercase()
                    } else {
                        '_'
                    }
                })
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
/// `Debug` prints positions, kinds and sources, never the generalized text.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
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
    /// "employer", "payer", "recipient", "lender", "borrower", "taxpayer",
    /// "spouse", "dependent", "designee", "preparer", "requester".
    pub role: Option<String>,
    /// Replacement text for [`Treatment::Generalize`], e.g. "Brooklyn, NY".
    /// `None` for every other treatment.
    #[serde(default)]
    pub generalized: Option<String>,
    /// When set, `span` indexes into the value of the form field with this
    /// name instead of a page's text, and `page` is ignored.
    #[serde(default)]
    pub field: Option<String>,
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
/// `Debug` prints the page number and text length, never the text.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct PageText {
    /// 1-based.
    pub page: u32,
    pub text: String,
    pub origin: TextOrigin,
}

/// A fillable form field (AcroForm widget) with its value.
/// `Debug` prints the field name and value length, never the value.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct FormField {
    pub name: String,
    pub value: String,
    pub page: Option<u32>,
}

/// Everything extraction produces for one file. `Debug` is redacted.
#[derive(Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Extracted {
    pub pages: Vec<PageText>,
    pub fields: Vec<FormField>,
}

impl fmt::Debug for Extracted {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Extracted")
            .field("pages", &self.pages)
            .field("fields", &self.fields)
            .finish()
    }
}

impl fmt::Debug for PageText {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PageText")
            .field("page", &self.page)
            .field("len", &self.text.len())
            .field("origin", &self.origin)
            .finish()
    }
}

impl fmt::Debug for FormField {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FormField")
            .field("name", &self.name)
            .field("value_len", &self.value.len())
            .field("page", &self.page)
            .finish()
    }
}

impl fmt::Debug for Detection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Detection")
            .field("page", &self.page)
            .field("span", &self.span)
            .field("kind", &self.kind)
            .field("source", &self.source)
            .field("confidence", &self.confidence)
            .field("treatment", &self.treatment)
            .field("needs_review", &self.needs_review)
            .field("has_review_prompt", &self.review_prompt.is_some())
            .field("role", &self.role)
            .field(
                "generalized_len",
                &self.generalized.as_ref().map(String::len),
            )
            .field("field", &self.field)
            .finish()
    }
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

    /// Legal state machine moves. Staying in the same state is always allowed
    /// (status note updates).
    ///
    /// Queued -> Reading -> Inking -> NeedsAttention | Ready | Failed;
    /// NeedsAttention -> Inking (after an answer); Queued | Reading -> Failed;
    /// Failed -> Queued (retry); Reading | Inking -> Queued (recovery after
    /// the process died mid-processing); Ready | NeedsAttention -> Queued
    /// ("Ink again").
    pub fn can_transition_to(self, to: DocState) -> bool {
        use DocState::*;
        self == to
            || matches!(
                (self, to),
                (Queued, Reading)
                    | (Reading, Inking)
                    | (Inking, NeedsAttention)
                    | (Inking, Ready)
                    | (Inking, Failed)
                    | (NeedsAttention, Inking)
                    | (Queued, Failed)
                    | (Reading, Failed)
                    | (Failed, Queued)
                    | (Reading, Queued)
                    | (Inking, Queued)
                    | (Ready, Queued)
                    | (NeedsAttention, Queued)
            )
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
    /// The chip the document counts under ("Taxes", "Banking"), from its
    /// template or kind. `None` shows under `All` only.
    #[serde(default)]
    pub tag: Option<String>,
    /// The token of the employer, payer or lender a template attributed,
    /// e.g. `[ORG_3]`. The popover reads the name back through the vault;
    /// the index itself holds only the token.
    #[serde(default)]
    pub payer_token: Option<Token>,
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

/// What `resolve` returns: a document's tokenized text or a folder listing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Resolved {
    Document(ResolvedDocument),
    Folder(ResolvedFolder),
}

/// Result of `write`: where the rehydrated file landed and the handle for its
/// tokenized view (decision 5.6). The path is for the shell, not the agent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WriteOutcome {
    pub path: Utf8PathBuf,
    pub handle: Handle,
    /// Tokens left as literal text because they are not from the source
    /// document (or folder) or are unknown.
    #[serde(default)]
    pub skipped_tokens: u32,
}

/// Identity of a review question (one attention-card item).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct QuestionId(pub i64);

/// One pending or answered attention-card question. Never contains the value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReviewQuestion {
    pub id: QuestionId,
    pub document: DocumentId,
    pub page: u32,
    pub kind: TokenKind,
    pub prompt: Option<String>,
    pub answer: Option<ReviewAnswer>,
    pub created_at: DateTime<Utc>,
    pub answered_at: Option<DateTime<Utc>>,
    /// Characters in the held span: the size of the card's dotted outline.
    #[serde(default)]
    pub value_length: u32,
    /// Up to 24 characters of the text just before the held span on its
    /// page, trimmed to a word boundary (a cut is marked with a leading `…`),
    /// with any run of 4+ digits replaced by `…`. Empty when unknown.
    #[serde(default)]
    pub context_before: String,
    /// The same, just after the span (a cut is marked with a trailing `…`).
    #[serde(default)]
    pub context_after: String,
    /// Pages in the document, for "page 3 of 11". 0 when unknown.
    #[serde(default)]
    pub page_count: u32,
    /// Where the held value sits in its page's text (a byte range), so the
    /// card can show a crop of the page around it. `None` when the value is
    /// a form field's, which has no place on the page, or was never recorded.
    #[serde(default)]
    pub span: Option<Span>,
}

/// One audit-log row: an agent read a handle. Feeds the "Claude Code read
/// 🐙/w2-acme-2025, saw only ink · 2m" footer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentRead {
    pub agent: String,
    pub handle: Handle,
    pub at: DateTime<Utc>,
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
    fn codename_deserialize_validates() {
        let ok: Codename = serde_json::from_str("\"w2-acme-2025\"").unwrap();
        assert_eq!(ok.as_str(), "w2-acme-2025");
        assert_eq!(serde_json::to_string(&ok).unwrap(), "\"w2-acme-2025\"");
        assert!(serde_json::from_str::<Codename>("\"Bad Name\"").is_err());
        assert!(serde_json::from_str::<Handle>("\"a--b\"").is_err());
    }

    #[test]
    fn doc_state_transitions() {
        use DocState::*;
        let legal = [
            (Queued, Reading),
            (Reading, Inking),
            (Inking, NeedsAttention),
            (Inking, Ready),
            (Inking, Failed),
            (NeedsAttention, Inking),
            (Queued, Failed),
            (Reading, Failed),
            (Failed, Queued),
            (Inking, Inking),
            // Recovery after a crash mid-processing.
            (Reading, Queued),
            (Inking, Queued),
            // "Ink again" on the row menu.
            (Ready, Queued),
            (NeedsAttention, Queued),
        ];
        for (a, b) in legal {
            assert!(a.can_transition_to(b), "{a:?} -> {b:?}");
        }
        let illegal = [
            (Queued, Ready),
            (Queued, Inking),
            (Reading, Ready),
            (NeedsAttention, Ready),
            (Ready, Inking),
            (Ready, Reading),
            (Failed, Ready),
        ];
        for (a, b) in illegal {
            assert!(!a.can_transition_to(b), "{a:?} -> {b:?}");
        }
    }

    #[test]
    fn ids_parse_their_own_string_form_only() {
        let d = DocumentId::new();
        assert_eq!(DocumentId::parse(&d.0.to_string()), Some(d));
        assert_eq!(DocumentId::parse(" d-1 "), None);
        let f = FolderId::new();
        assert_eq!(FolderId::parse(&format!(" {} ", f.0)), Some(f));
    }

    #[test]
    fn detection_new_fields_default() {
        let json = r#"{"page":1,"span":{"start":0,"end":3},"kind":"ssn",
            "source":{"source":"rule","name":"ssn"},"confidence":0.9,
            "treatment":"tokenize","needs_review":false,"review_prompt":null,"role":null}"#;
        let d: Detection = serde_json::from_str(json).unwrap();
        assert_eq!(d.generalized, None);
        assert_eq!(d.field, None);
    }

    #[test]
    fn resolved_serializes_tagged() {
        let f = ResolvedFolder {
            handle: Handle::parse("ink/taxes-2025").unwrap(),
            children: vec![],
        };
        let v = serde_json::to_value(Resolved::Folder(f)).unwrap();
        assert_eq!(v["type"], "folder");
    }

    #[test]
    fn small_helpers() {
        let e = Extracted {
            pages: vec![
                PageText {
                    page: 1,
                    text: "a".into(),
                    origin: TextOrigin::TextLayer,
                },
                PageText {
                    page: 2,
                    text: "b".into(),
                    origin: TextOrigin::Ocr,
                },
            ],
            fields: vec![],
        };
        assert_eq!(e.full_text(), "a\n\nb");
        assert!(DocState::Ready.is_servable());
        assert!(!DocState::NeedsAttention.is_servable());
        assert_eq!(Span { start: 2, end: 5 }.len(), 3);
        assert!(Span { start: 5, end: 5 }.is_empty());
        assert_eq!(DocKind::unknown().as_str(), "unknown");
        assert_ne!(DocumentId::new(), DocumentId::default());
        assert_ne!(FolderId::new(), FolderId::default());
    }

    #[test]
    fn debug_never_prints_text_or_values() {
        let secret = "Testy McFakename 000-12-3456";
        let e = Extracted {
            pages: vec![PageText {
                page: 3,
                text: secret.into(),
                origin: TextOrigin::Ocr,
            }],
            fields: vec![FormField {
                name: "f1_01".into(),
                value: secret.into(),
                page: Some(1),
            }],
        };
        let d = Detection {
            page: 2,
            span: Span { start: 4, end: 9 },
            kind: TokenKind::Person,
            source: DetectionSource::Rule { name: "r".into() },
            confidence: 0.9,
            treatment: Treatment::Generalize,
            needs_review: false,
            review_prompt: None,
            role: Some("employee".into()),
            generalized: Some(secret.into()),
            field: Some("f1_01".into()),
        };
        for out in [
            format!("{e:?}"),
            format!("{:?}", e.pages[0]),
            format!("{:?}", e.fields[0]),
            format!("{d:?}"),
            format!("{e:#?}"),
        ] {
            assert!(!out.contains("McFakename"), "{out}");
            assert!(!out.contains("000-12-3456"), "{out}");
        }
        let p = format!("{:?}", e.pages[0]);
        assert!(
            p.contains("page: 3") && p.contains(&format!("len: {}", secret.len())),
            "{p}"
        );
        let f = format!("{:?}", e.fields[0]);
        assert!(f.contains(&format!("value_len: {}", secret.len())), "{f}");
        let ds = format!("{d:?}");
        assert!(
            ds.contains("Person") && ds.contains("start: 4") && ds.contains("page: 2"),
            "{ds}"
        );
    }

    #[test]
    fn write_outcome_skipped_defaults() {
        let o: WriteOutcome =
            serde_json::from_str(r#"{"path":"/p/a.md","handle":"w2-2025-a"}"#).unwrap();
        assert_eq!(o.skipped_tokens, 0);
    }

    #[test]
    fn review_question_peek_fields_default() {
        let q: ReviewQuestion = serde_json::from_str(
            r#"{"id":4,"document":"00000000-0000-0000-0000-000000000000","page":3,
            "kind":"account","prompt":null,"answer":null,
            "created_at":"2026-01-01T00:00:00Z","answered_at":null}"#,
        )
        .unwrap();
        assert_eq!(q.value_length, 0);
        assert_eq!(q.page_count, 0);
        assert!(q.context_before.is_empty() && q.context_after.is_empty());
        assert_eq!(q.span, None);
    }

    #[test]
    fn token_renders_like_the_brief() {
        let t = Token {
            kind: TokenKind::Ssn,
            index: 1,
        };
        assert_eq!(t.to_string(), "[SSN_1]");
        let t = Token {
            kind: TokenKind::Custom("plan id".into()),
            index: 3,
        };
        assert_eq!(t.to_string(), "[PLAN_ID_3]");
    }
}

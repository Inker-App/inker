//! Pack, manifest and form template types, and the loader.

use std::collections::BTreeMap;
use std::path::Path;

use inker_core::{DocKind, TokenKind, Treatment};
use regex::Regex;
use serde::Deserialize;

use crate::{PackError, Result};

/// Roles a template field may assign, as `describe` reports them.
pub const ROLES: &[&str] = &[
    "employee",
    "employer",
    "payer",
    "recipient",
    "lender",
    "borrower",
];

/// The tax pack, embedded at build time from `packs/tax` at the repo root.
static TAX_PACK: include_dir::Dir<'static> =
    include_dir::include_dir!("$CARGO_MANIFEST_DIR/../../../packs/tax");

/// Every pack in use. Detection walks the forms of all packs.
#[derive(Debug, Clone, Default)]
pub struct PackSet {
    pub packs: Vec<Pack>,
}

/// One loaded pack.
#[derive(Debug, Clone)]
pub struct Pack {
    pub manifest: Manifest,
    pub forms: Vec<FormTemplate>,
}

/// `pack.toml`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: Option<String>,
    /// File stems under `forms/`, e.g. `w2` for `forms/w2.toml`.
    pub forms: Vec<String>,
    #[serde(default)]
    pub codename: CodenameRules,
}

/// Codename rules: the short form id used as the first codename segment.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CodenameRules {
    /// Used when a kind has no entry in `short`.
    #[serde(default)]
    pub default: Option<String>,
    /// `DocKind` string -> short id, e.g. `1099-int` -> `1099int`.
    #[serde(default)]
    pub short: BTreeMap<String, String>,
}

/// What a template field holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldKind {
    /// Leave the value as is (amounts, box numbers, dates, the state).
    Keep,
    /// A sensitive value of this kind.
    Token(TokenKind),
}

/// How the value after a label is shaped, which decides how much text the
/// engine takes as the value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValueShape {
    /// The first column segment of the rest of the line, or of the next line.
    Line,
    /// `lines` whole lines (addresses).
    Lines,
    /// The first SSN, ITIN or EIN shaped value (masked ones included).
    Tin,
    /// The first run of letters and digits holding at least four digits.
    Account,
    /// The first nine-digit run.
    Routing,
    /// The first date (`MM/DD/YYYY`, `MM-DD-YYYY` or `YYYY-MM-DD`), e.g. a
    /// date of birth in a table row.
    Date,
}

/// One box on a form.
#[derive(Debug, Clone)]
pub struct FieldSpec {
    /// Box id as printed, e.g. `a`, `1`, `12a`, or a name like `payer_name`.
    pub id: String,
    /// Case-insensitive regex for the fixed IRS wording before the value.
    pub label: Regex,
    pub kind: FieldKind,
    pub treatment: Treatment,
    pub role: Option<String>,
    /// AcroForm field names (fillable IRS PDFs), compared case-insensitively
    /// against the full name and its last segment without `[n]` indices.
    pub acroform: Vec<String>,
    pub value: ValueShape,
    /// Non-empty lines to skip after the first value line (an address under a name).
    pub skip_lines: usize,
    /// Lines to take for `ValueShape::Lines`.
    pub lines: usize,
}

/// A form template, from `forms/<id>.toml`.
#[derive(Debug, Clone)]
pub struct FormTemplate {
    pub id: String,
    pub display: String,
    /// `DocKind` string, e.g. `w2`, `1099-int`.
    pub kind: String,
    /// Every anchor must match a page for the template to apply to it.
    pub anchors: Vec<Regex>,
    /// Capture group 1 is the tax year.
    pub year: Option<Regex>,
    pub fields: Vec<FieldSpec>,
    /// File the template came from, for error messages.
    pub file: String,
}

impl PackSet {
    /// The embedded tax pack.
    pub fn builtin() -> Result<Self> {
        let manifest = TAX_PACK
            .get_file("pack.toml")
            .and_then(|f| f.contents_utf8())
            .ok_or_else(|| invalid("pack.toml", "embedded tax pack has no pack.toml"))?;
        let mut forms = Vec::new();
        if let Some(dir) = TAX_PACK.get_dir("forms") {
            for f in dir.files() {
                let path = f.path();
                if path.extension().and_then(|e| e.to_str()) != Some("toml") {
                    continue;
                }
                let stem = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or_default();
                let text = f
                    .contents_utf8()
                    .ok_or_else(|| invalid(&path.display().to_string(), "not UTF-8"))?;
                forms.push((stem, text));
            }
        }
        Ok(Self {
            packs: vec![Pack::from_sources(manifest, &forms)?],
        })
    }

    /// Load one pack from a directory on disk.
    pub fn load_dir(dir: &Path) -> Result<Self> {
        let read = |p: &Path| {
            std::fs::read_to_string(p).map_err(|source| PackError::Io {
                path: p.display().to_string(),
                source,
            })
        };
        let manifest = read(&dir.join("pack.toml"))?;
        let forms_dir = dir.join("forms");
        let mut texts = Vec::new();
        if forms_dir.is_dir() {
            let entries = std::fs::read_dir(&forms_dir).map_err(|source| PackError::Io {
                path: forms_dir.display().to_string(),
                source,
            })?;
            for entry in entries {
                let path = entry
                    .map_err(|source| PackError::Io {
                        path: forms_dir.display().to_string(),
                        source,
                    })?
                    .path();
                if path.extension().and_then(|e| e.to_str()) != Some("toml") {
                    continue;
                }
                let stem = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or_default()
                    .to_string();
                texts.push((stem, read(&path)?));
            }
        }
        let forms: Vec<(&str, &str)> = texts
            .iter()
            .map(|(a, b)| (a.as_str(), b.as_str()))
            .collect();
        Ok(Self {
            packs: vec![Pack::from_sources(&manifest, &forms)?],
        })
    }

    /// The override directory if given, else the embedded tax pack.
    pub fn load(override_dir: Option<&Path>) -> Result<Self> {
        match override_dir {
            Some(dir) => Self::load_dir(dir),
            None => Self::builtin(),
        }
    }

    /// Every form template across packs, in pack then manifest order.
    pub fn forms(&self) -> impl Iterator<Item = &FormTemplate> {
        self.packs.iter().flat_map(|p| p.forms.iter())
    }

    /// Short codename segment for a document kind, e.g. `1099int`.
    pub fn codename_short(&self, kind: &DocKind) -> Option<&str> {
        self.packs
            .iter()
            .find_map(|p| p.manifest.codename.short.get(kind.as_str()))
            .or_else(|| {
                self.packs
                    .iter()
                    .find_map(|p| p.manifest.codename.default.as_ref())
            })
            .map(String::as_str)
    }
}

impl Pack {
    /// Parse a pack from the manifest text and `(file stem, text)` pairs for
    /// its forms. Every stem listed in the manifest must be present; files
    /// not listed are ignored.
    pub fn from_sources(manifest: &str, forms: &[(&str, &str)]) -> Result<Self> {
        let manifest: Manifest =
            toml::from_str(manifest).map_err(|e| toml_error("pack.toml", manifest, &e))?;
        let mut out: Vec<FormTemplate> = Vec::new();
        for stem in &manifest.forms {
            let file = format!("forms/{stem}.toml");
            let (_, text) = forms
                .iter()
                .find(|(s, _)| s == stem)
                .ok_or_else(|| invalid("pack.toml", &format!("lists {file}, which is missing")))?;
            let t = FormTemplate::parse(&file, text)?;
            if out.iter().any(|o| o.id == t.id) {
                return Err(invalid(&file, &format!("duplicate template id {}", t.id)));
            }
            out.push(t);
        }
        tracing::debug!(pack = %manifest.name, forms = out.len(), "pack loaded");
        Ok(Self {
            manifest,
            forms: out,
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawTemplate {
    id: String,
    display: String,
    kind: String,
    anchors: Vec<String>,
    #[serde(default)]
    year: Option<String>,
    #[serde(default)]
    fields: Vec<RawField>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawField {
    id: String,
    label: String,
    kind: String,
    treatment: Treatment,
    #[serde(default)]
    role: Option<String>,
    #[serde(default)]
    acroform: Vec<String>,
    #[serde(default)]
    value: Option<ValueShape>,
    #[serde(default)]
    skip_lines: usize,
    #[serde(default)]
    lines: Option<usize>,
}

fn toml_error(file: &str, text: &str, e: &toml::de::Error) -> PackError {
    let line = e
        .span()
        .map(|s| text[..s.start.min(text.len())].matches('\n').count() + 1);
    PackError::Parse {
        file: file.into(),
        message: match line {
            Some(l) => format!("line {l}: {}", e.message()),
            None => e.message().to_string(),
        },
    }
}

fn invalid(file: &str, message: &str) -> PackError {
    PackError::Invalid {
        file: file.into(),
        message: message.into(),
    }
}

fn compile(file: &str, field: &str, pattern: &str) -> Result<Regex> {
    Regex::new(&format!("(?i){pattern}")).map_err(|e| PackError::Regex {
        file: file.into(),
        field: field.into(),
        message: e.to_string(),
    })
}

impl FormTemplate {
    /// Parse and validate one template. `file` names it in errors.
    pub fn parse(file: &str, text: &str) -> Result<Self> {
        let raw: RawTemplate = toml::from_str(text).map_err(|e| toml_error(file, text, &e))?;
        if raw.anchors.is_empty() {
            return Err(invalid(file, "a template needs at least one anchor"));
        }
        let anchors = raw
            .anchors
            .iter()
            .map(|a| compile(file, "anchors", a))
            .collect::<Result<Vec<_>>>()?;
        let year = match &raw.year {
            Some(y) => {
                let re = compile(file, "year", y)?;
                if re.captures_len() < 2 {
                    return Err(invalid(
                        file,
                        "year regex needs a capture group for the year",
                    ));
                }
                Some(re)
            }
            None => None,
        };
        let mut fields: Vec<FieldSpec> = Vec::with_capacity(raw.fields.len());
        for f in raw.fields {
            if fields.iter().any(|o| o.id == f.id) {
                return Err(invalid(file, &format!("duplicate field id {}", f.id)));
            }
            let kind = parse_field_kind(&f.kind);
            match (&kind, f.treatment) {
                (FieldKind::Keep, Treatment::Keep) => {}
                (FieldKind::Keep, _) => {
                    return Err(invalid(
                        file,
                        &format!("field {}: kind keep needs treatment keep", f.id),
                    ));
                }
                (FieldKind::Token(_), Treatment::Keep) => {
                    return Err(invalid(
                        file,
                        &format!("field {}: a sensitive kind cannot use treatment keep", f.id),
                    ));
                }
                _ => {}
            }
            if let Some(role) = &f.role
                && !ROLES.contains(&role.as_str())
            {
                return Err(invalid(
                    file,
                    &format!("field {}: unknown role {role}", f.id),
                ));
            }
            let value = f.value.unwrap_or(match &kind {
                FieldKind::Token(TokenKind::Ssn | TokenKind::Itin | TokenKind::Ein) => {
                    ValueShape::Tin
                }
                FieldKind::Token(TokenKind::Account) => ValueShape::Account,
                FieldKind::Token(TokenKind::Routing) => ValueShape::Routing,
                FieldKind::Token(TokenKind::Address) => ValueShape::Lines,
                _ => ValueShape::Line,
            });
            let lines = f
                .lines
                .unwrap_or(if value == ValueShape::Lines { 2 } else { 1 });
            if lines == 0 || lines > 6 {
                return Err(invalid(
                    file,
                    &format!("field {}: lines must be 1 to 6", f.id),
                ));
            }
            fields.push(FieldSpec {
                label: compile(file, &f.id, &f.label)?,
                id: f.id,
                kind,
                treatment: f.treatment,
                role: f.role,
                acroform: f.acroform,
                value,
                skip_lines: f.skip_lines,
                lines,
            });
        }
        Ok(Self {
            id: raw.id,
            display: raw.display,
            kind: raw.kind,
            anchors,
            year,
            fields,
            file: file.into(),
        })
    }
}

/// Parse a `kind` string from a template: a `TokenKind` in snake_case, or
/// `keep`. Unknown names become `TokenKind::Custom`.
pub fn parse_field_kind(s: &str) -> FieldKind {
    let k = match s {
        "keep" => return FieldKind::Keep,
        "ssn" => TokenKind::Ssn,
        "itin" => TokenKind::Itin,
        "ein" => TokenKind::Ein,
        "account" => TokenKind::Account,
        "routing" => TokenKind::Routing,
        "card" => TokenKind::Card,
        "person" => TokenKind::Person,
        "org" => TokenKind::Org,
        "address" => TokenKind::Address,
        "email" => TokenKind::Email,
        "phone" => TokenKind::Phone,
        "gov_id" => TokenKind::GovId,
        other => TokenKind::Custom(other.to_string()),
    };
    FieldKind::Token(k)
}

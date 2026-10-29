//! Document classification: kind, tax year and the employer or payer name.

use std::sync::LazyLock;

use inker_core::{Detection, DetectionSource, DocKind, Extracted, TextOrigin, TokenKind};
use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::template::TemplateMatch;
use crate::util::re;

/// What a document is. `org` is the employer or payer name as written; the
/// core uses it for the codename and never shows it to an agent. `Debug`
/// prints `<redacted>` in its place.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Classification {
    pub kind: DocKind,
    pub year: Option<u16>,
    pub org: Option<String>,
    /// The popover chip ("Taxes", "Banking", "Legal"): the template's tag,
    /// or the one a heuristic kind implies. `None` shows under `All` only.
    #[serde(default)]
    pub tag: Option<String>,
}

impl std::fmt::Debug for Classification {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Classification")
            .field("kind", &self.kind)
            .field("year", &self.year)
            .field("org", &self.org.as_ref().map(|_| Redacted))
            .field("tag", &self.tag)
            .finish()
    }
}

struct Redacted;

impl std::fmt::Debug for Redacted {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("<redacted>")
    }
}

pub const BANK_STATEMENT: &str = "bank-statement";
pub const CLOSING_STATEMENT: &str = "closing-statement";
pub const RECEIPT: &str = "receipt";
/// An IRS form with no template: a form number plus an OMB control number.
pub const TAX_FORM: &str = "tax-form";

/// Roles whose organisation names the document (employer or payer).
const ORG_ROLES: &[&str] = &["employer", "payer", "lender"];

static FORM_NO: LazyLock<Regex> =
    LazyLock::new(|| re(r"\bForm\s+(?:1099-[A-Z]{1,3}|W-2[A-Z]?|1098(?:-[A-Z])?)\b"));
static OMB: LazyLock<Regex> = LazyLock::new(|| re(r"\bOMB\s+No\.?\s*\d{4}-\d{4}"));
static YEAR: LazyLock<Regex> = LazyLock::new(|| re(r"(?:19[89]\d|20\d{2})"));

/// Classify from the matched template if any, else from the text.
pub fn classify(
    extracted: &Extracted,
    matched: Option<&TemplateMatch<'_>>,
    detections: &[Detection],
) -> Classification {
    let org = org_name(extracted, detections);
    if let Some(m) = matched {
        return Classification {
            kind: DocKind(m.template.kind.clone()),
            year: m.year.or_else(|| common_year(extracted)),
            org,
            tag: m.template.tag.clone(),
        };
    }
    let kind = heuristic_kind(extracted);
    Classification {
        kind: DocKind(kind.into()),
        year: common_year(extracted),
        org,
        tag: heuristic_tag(kind).map(str::to_string),
    }
}

/// The chip a kind without a template counts under. Receipts, scans and
/// unknowns have no chip: they show under `All` only.
fn heuristic_tag(kind: &str) -> Option<&'static str> {
    match kind {
        BANK_STATEMENT => Some("Banking"),
        CLOSING_STATEMENT => Some("Legal"),
        TAX_FORM => Some("Taxes"),
        _ => None,
    }
}

fn heuristic_kind(extracted: &Extracted) -> &'static str {
    let lower = extracted.full_text().to_ascii_lowercase();
    let any = |words: &[&str]| words.iter().any(|w| lower.contains(w));
    let full = extracted.full_text();
    if FORM_NO.is_match(&full) && OMB.is_match(&full) {
        return TAX_FORM;
    }
    if any(&[
        "closing disclosure",
        "settlement statement",
        "closing statement",
        "hud-1",
    ]) {
        CLOSING_STATEMENT
    } else if any(&[
        "statement period",
        "account summary",
        "beginning balance",
        "opening balance",
    ]) && any(&[
        "ending balance",
        "closing balance",
        "deposits",
        "withdrawals",
    ]) {
        BANK_STATEMENT
    } else if any(&["receipt"]) || (any(&["subtotal"]) && any(&["total"])) {
        RECEIPT
    } else if !extracted.pages.is_empty()
        && extracted.pages.iter().all(|p| p.origin == TextOrigin::Ocr)
    {
        DocKind::SCAN
    } else {
        DocKind::UNKNOWN
    }
}

/// The most common four-digit year that stands alone (not inside a longer
/// number); ties go to the first seen.
fn common_year(extracted: &Extracted) -> Option<u16> {
    let mut counts: Vec<(u16, usize)> = Vec::new();
    for p in &extracted.pages {
        let b = p.text.as_bytes();
        for m in YEAR.find_iter(&p.text) {
            let before = m.start().checked_sub(1).map(|i| b[i]);
            let after = b.get(m.end()).copied();
            if before.is_some_and(|c| c.is_ascii_digit())
                || after.is_some_and(|c| c.is_ascii_digit())
            {
                continue;
            }
            let Ok(y) = m.as_str().parse::<u16>() else {
                continue;
            };
            match counts.iter_mut().find(|(v, _)| *v == y) {
                Some((_, n)) => *n += 1,
                None => counts.push((y, 1)),
            }
        }
    }
    let best = counts.iter().map(|(_, n)| *n).max()?;
    counts.iter().find(|(_, n)| *n == best).map(|(y, _)| *y)
}

/// The employer, payer or lender name, read from a template field with that
/// role. Never from the names pass or fail-closed: the core puts this in the
/// codename, so a guess must not reach it.
fn org_name(extracted: &Extracted, detections: &[Detection]) -> Option<String> {
    let pick = detections
        .iter()
        .filter(|d| {
            d.kind == TokenKind::Org
                && matches!(d.source, DetectionSource::Template { .. })
                && d.role.as_deref().is_some_and(|r| ORG_ROLES.contains(&r))
        })
        .min_by_key(|d| (d.field.is_none(), d.page, d.span.start))?;
    let source = match &pick.field {
        Some(name) => extracted
            .fields
            .iter()
            .find(|f| &f.name == name)
            .map(|f| f.value.as_str()),
        None => extracted
            .pages
            .iter()
            .find(|p| p.page == pick.page)
            .map(|p| p.text.as_str()),
    }?;
    let value = source.get(pick.span.start..pick.span.end)?.trim();
    (!value.is_empty()).then(|| value.to_string())
}

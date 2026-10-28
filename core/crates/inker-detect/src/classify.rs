//! Document classification: kind, tax year and the employer or payer name.

use std::sync::LazyLock;

use inker_core::{Detection, DocKind, Extracted, TextOrigin, TokenKind};
use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::template::TemplateMatch;
use crate::util::re;

/// What a document is. `org` is the employer or payer name as written; the
/// core uses it for the codename and never shows it to an agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Classification {
    pub kind: DocKind,
    pub year: Option<u16>,
    pub org: Option<String>,
}

pub const BANK_STATEMENT: &str = "bank-statement";
pub const CLOSING_STATEMENT: &str = "closing-statement";
pub const RECEIPT: &str = "receipt";

/// Roles whose organisation names the document (employer or payer).
const ORG_ROLES: &[&str] = &["employer", "payer", "lender"];

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
        };
    }
    Classification {
        kind: DocKind(heuristic_kind(extracted).into()),
        year: common_year(extracted),
        org,
    }
}

fn heuristic_kind(extracted: &Extracted) -> &'static str {
    let lower = extracted.full_text().to_ascii_lowercase();
    let any = |words: &[&str]| words.iter().any(|w| lower.contains(w));
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

/// The employer, payer or lender name; else the first organisation.
fn org_name(extracted: &Extracted, detections: &[Detection]) -> Option<String> {
    let mut orgs: Vec<&Detection> = detections
        .iter()
        .filter(|d| d.kind == TokenKind::Org)
        .collect();
    orgs.sort_by_key(|d| (d.field.is_none(), d.page, d.span.start));
    let pick = orgs
        .iter()
        .find(|d| d.role.as_deref().is_some_and(|r| ORG_ROLES.contains(&r)))
        .or_else(|| orgs.first())?;
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

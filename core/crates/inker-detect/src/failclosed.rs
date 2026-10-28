//! The fail-closed pass. Runs last, over whatever the other stages left:
//! any long digit run, or any capitalised run close after a sensitive label,
//! is inked and the document is held for the attention card. OCR pages lower
//! the digit threshold, widen the name window and read O/o as 0 and I/l as 1
//! inside digit runs.
//!
//! Prompts never contain the value.

use std::sync::LazyLock;

use inker_core::{Detection, DetectionSource, PageText, TextOrigin, TokenKind, Treatment};
use regex::Regex;

use crate::names;
use crate::rules::{ACCOUNT_LABELS, EIN_LABELS, ROUTING_LABELS, SSN_LABELS};
use crate::util::{detection, label_before, overlaps, re};

/// Minimum digits in a leftover run on a text-layer page and on an OCR page.
pub const DIGITS_TEXT: usize = 9;
pub const DIGITS_OCR: usize = 8;
/// How far after a sensitive label a name may start, by origin.
pub const NAME_WINDOW_TEXT: usize = 40;
pub const NAME_WINDOW_OCR: usize = 60;

/// Labels after which a capitalised run is treated as a possible name.
pub const NAME_LABELS: &[&str] = &[
    "ssn",
    "social security",
    "tin",
    "ein",
    "account",
    "acct",
    "routing",
    "name",
];

static RUN_TEXT: LazyLock<Regex> = LazyLock::new(|| re(r"\d(?:[- ]?\d)*"));
static RUN_OCR: LazyLock<Regex> = LazyLock::new(|| re(r"\d(?:[- ]?[\dOoIl])*"));
static DATE: LazyLock<Regex> = LazyLock::new(|| re(r"^(?:\d{4}-\d{2}-\d{2}|\d{2}-\d{2}-\d{4})$"));

/// What a leftover number most likely is, from the nearest label before it.
fn number_kind(
    lower: &str,
    start: usize,
    window: usize,
) -> (TokenKind, Option<&'static str>, &'static str) {
    let options: [(&[&str], TokenKind, &str, &str); 4] = [
        (
            ROUTING_LABELS,
            TokenKind::Routing,
            "a routing number",
            "routing",
        ),
        (
            SSN_LABELS,
            TokenKind::Ssn,
            "a Social Security number",
            "ssn",
        ),
        (EIN_LABELS, TokenKind::Ein, "an employer ID number", "ein"),
        (
            ACCOUNT_LABELS,
            TokenKind::Account,
            "an account number",
            "account",
        ),
    ];
    options
        .into_iter()
        .filter_map(|(labels, kind, what, why)| {
            label_before(lower, start, window, labels).map(|d| (d, kind, what, why))
        })
        .min_by_key(|(d, ..)| *d)
        .map_or(
            (TokenKind::Custom("number".into()), None, "long_number"),
            |(_, k, what, why)| (k, Some(what), why),
        )
}

fn prompt(what: Option<&str>, ocr: bool) -> String {
    match (what, ocr) {
        (Some(w), false) => format!("Looks like {w}, but I'm not sure. Ink it?"),
        (Some(w), true) => format!("Looks like {w}, but the scan is rough. Ink it?"),
        (None, false) => "A long number I can't place. Ink it?".into(),
        (None, true) => "A long number, and the scan is rough. Ink it?".into(),
    }
}

fn held(
    page: u32,
    start: usize,
    end: usize,
    kind: TokenKind,
    reason: &str,
    conf: f32,
    prompt: String,
) -> Detection {
    let source = DetectionSource::FailClosed {
        reason: reason.into(),
    };
    let mut d = detection(page, start, end, kind, source, conf, Treatment::Tokenize);
    d.needs_review = true;
    d.review_prompt = Some(prompt);
    d
}

/// Split a run at spaces unless every space-separated group has four
/// characters (card style). Returns byte ranges relative to the run.
fn pieces(run: &str) -> Vec<(usize, usize)> {
    let groups: Vec<&str> = run.split(' ').collect();
    if groups.len() == 1 || groups.iter().all(|g| g.len() == 4) {
        return vec![(0, run.len())];
    }
    let mut out = Vec::new();
    let mut off = 0;
    for g in groups {
        out.push((off, off + g.len()));
        off += g.len() + 1;
    }
    out
}

/// True when the run is part of an amount: `$` before it, or a decimal or
/// thousands separator glued to a digit on either side.
fn looks_like_amount(text: &str, start: usize, end: usize) -> bool {
    let b = text.as_bytes();
    let before = text[..start].trim_end_matches(' ');
    let after_sep =
        matches!(b.get(end), Some(b'.' | b',')) && b.get(end + 1).is_some_and(u8::is_ascii_digit);
    let before_sep =
        start >= 2 && matches!(b[start - 1], b'.' | b',') && b[start - 2].is_ascii_digit();
    before.ends_with('$') || after_sep || before_sep
}

/// Leftover long numbers and names near sensitive labels on one page.
/// `covered` holds byte spans already detected or kept on this page.
pub fn find(page: &PageText, covered: &[(usize, usize)]) -> Vec<Detection> {
    let ocr = page.origin == TextOrigin::Ocr;
    let text = page.text.as_str();
    let lower = text.to_ascii_lowercase();
    let (min_digits, window, conf) = if ocr {
        (DIGITS_OCR, NAME_WINDOW_OCR, 0.35)
    } else {
        (DIGITS_TEXT, NAME_WINDOW_TEXT, 0.4)
    };
    let mut out: Vec<Detection> = Vec::new();
    let mut taken: Vec<(usize, usize)> = covered.to_vec();

    let runs = if ocr { &*RUN_OCR } else { &*RUN_TEXT };
    for m in runs.find_iter(text) {
        // Trim trailing separators or misread letters back to a digit.
        let run = m.as_str().trim_end_matches(|c: char| !c.is_ascii_digit());
        for (s, e) in pieces(run) {
            let (start, end) = (m.start() + s, m.start() + e);
            let piece = &text[start..end];
            let digits = piece.chars().filter(|c| c.is_ascii_alphanumeric()).count();
            let real = piece.chars().filter(char::is_ascii_digit).count();
            if digits < min_digits
                || real + 2 < min_digits
                || DATE.is_match(piece)
                || looks_like_amount(text, start, end)
                || overlaps(&taken, start, end)
            {
                continue;
            }
            let (kind, what, why) = number_kind(&lower, start, window);
            taken.push((start, end));
            out.push(held(
                page.page,
                start,
                end,
                kind,
                why,
                conf,
                prompt(what, ocr),
            ));
        }
    }

    let name_prompt = if ocr {
        "Looks like a name, but the scan is rough. Ink it?"
    } else {
        "Looks like a name, but I'm not sure. Ink it?"
    };
    let b = text.as_bytes();
    for label in NAME_LABELS {
        for (i, m) in lower.match_indices(label) {
            let label_end = i + m.len();
            let boundary = (i == 0 || !b[i - 1].is_ascii_alphanumeric())
                && b.get(label_end).is_none_or(|c| !c.is_ascii_alphabetic());
            if !boundary {
                continue;
            }
            // The first capitalised word within the window that starts a name.
            let limit = (label_end + window).min(text.len());
            let found = (label_end..limit)
                .filter(|&j| {
                    b[j].is_ascii_uppercase()
                        && (j == 0 || !b[j - 1].is_ascii_alphanumeric())
                        && text.is_char_boundary(j)
                })
                .find_map(|j| names::run_at(text, j));
            let Some(r) = found else { continue };
            if overlaps(&taken, r.start, r.end) {
                continue;
            }
            let kind = if r.org {
                TokenKind::Org
            } else {
                TokenKind::Person
            };
            taken.push((r.start, r.end));
            out.push(held(
                page.page,
                r.start,
                r.end,
                kind,
                "name_near_label",
                conf - 0.1,
                name_prompt.into(),
            ));
        }
    }
    out.sort_by_key(|d| d.span.start);
    out
}

//! Values on a template page that no other stage placed.
//!
//! An IRS text layer lists every label first and the typed values after, so
//! a value no rule recognises (a name without an org suffix, a control or
//! state ID number) cannot be tied to its box. On pages where a template
//! matched, this pass runs last and holds what is left that is shaped like a
//! value: a run of 2 to 4 capitalised words, a code mixing letters and
//! digits or joining digit groups with a dash, or a run of six or more
//! digits. Labels, anchors, the pack's boilerplate lines, revision dates and
//! the shapes in [`shape`] are left alone.
//!
//! Prompts never contain the value.

use std::sync::LazyLock;

use inker_core::{Detection, DetectionSource, PageText, TokenKind, Treatment};
use inker_packs::{FormTemplate, PackSet};
use regex::Regex;

use crate::address;
use crate::failclosed::held;
use crate::rules::ACCOUNT_LABELS;
use crate::shape;
use crate::template;
use crate::util::{detection, label_before, re, uncovered};

/// Fail-closed confidence for an unplaced value.
const CONF: f32 = 0.4;
/// How far before an account-shaped code an account label may end.
const ACCOUNT_WINDOW: usize = 24;
/// Words in a capitalised run that may be a name.
const NAME_WORDS: std::ops::RangeInclusive<usize> = 2..=4;
/// Last words that make a capitalised run an organisation.
const ORG_ENDINGS: &[&str] = &[
    "co", "company", "corp", "inc", "llc", "llp", "lp", "ltd", "bank", "union", "trust", "na",
];

/// The question for an unplaced name.
pub const NAME_PROMPT: &str = "Looks like a name on this form, but I can't place it. Ink it?";
/// The question for an unplaced number.
pub const NUMBER_PROMPT: &str = "A number on this form I can't place. Ink it?";

/// Letters, digits and single dashes between them.
static CODE: LazyLock<Regex> = LazyLock::new(|| re(r"^[A-Za-z0-9]+(?:-[A-Za-z0-9]+)*$"));
/// Digit groups joined by a dash, e.g. `0047-91234`.
static DASHED_DIGITS: LazyLock<Regex> = LazyLock::new(|| re(r"\d-\d"));
static LONG_DIGITS: LazyLock<Regex> = LazyLock::new(|| re(r"\d{6}"));
/// A ZIP or ZIP+4.
static ZIP: LazyLock<Regex> = LazyLock::new(|| re(r"^\d{5}(?:-\d{4})?$"));
/// An IRS form number: `1099-INT`, `1098`, `1095-C`, `1120-S`, `W-2`, `W-2c`.
static FORM: LazyLock<Regex> =
    LazyLock::new(|| re(r"^(?i:(?:1[01]\d\d(?:-?[A-Z]{1,4})?)|(?:W-?\d{1,2}[A-Z]{0,2}))$"));
/// A year glued to a word by text extraction ("2025Department").
static GLUED_YEAR: LazyLock<Regex> =
    LazyLock::new(|| re(r"^(?:(?:19|20)\d{2}[A-Z][a-z]{2,}|[A-Z][a-z]{2,}(?:19|20)\d{2})$"));
/// An account-shaped code: letters, then dashed digit groups.
static ACCOUNT_CODE: LazyLock<Regex> = LazyLock::new(|| re(r"^[A-Z]{2,}(?:-\d+){2,}$"));
/// Labels whose number is fixed form text, not a value.
const FIXED_NUMBER_LABELS: &[&str] = &["omb no.", "cat. no.", "omb no", "cat. no"];

/// Unplaced values on one page read against `template`. `covered` holds the
/// byte spans every other stage detected or kept on this page.
pub fn find(
    page: &PageText,
    template: &FormTemplate,
    packs: &PackSet,
    covered: &[(usize, usize)],
) -> Vec<Detection> {
    let text = page.text.as_str();
    let lower = text.to_ascii_lowercase();
    let mut skip: Vec<(usize, usize)> = covered.to_vec();
    skip.extend(template::label_spans(text, template));
    skip.extend(template::revisions(text));

    let mut out = Vec::new();
    let mut pos = 0;
    for line in text.split_inclusive('\n') {
        let ls = pos;
        pos += line.len();
        let line = line.trim_end_matches(['\n', '\r']);
        if packs.is_boilerplate(line) {
            continue;
        }
        for (ss, se) in segments(line) {
            let (ss, se) = (ls + ss, ls + se);
            if packs.is_boilerplate(&text[ss..se]) {
                continue;
            }
            for (ps, pe) in uncovered(ss, se, &skip) {
                let piece = &text[ps..pe];
                let lead = piece.len() - piece.trim_start().len();
                let (ps, pe) = (ps + lead, ps + lead + piece.trim().len());
                if ps >= pe || packs.is_boilerplate(&text[ps..pe]) {
                    continue;
                }
                if address::is_street_line(&text[ps..pe]) {
                    let source = DetectionSource::FailClosed {
                        reason: "street_line".into(),
                    };
                    out.push(detection(
                        page.page,
                        ps,
                        pe,
                        TokenKind::Address,
                        source,
                        CONF,
                        Treatment::Tokenize,
                    ));
                    continue;
                }
                if let Some(kind) = name_kind(&text[ps..pe]) {
                    out.push(held(
                        page.page,
                        ps,
                        pe,
                        kind,
                        "unplaced_name",
                        CONF - 0.1,
                        NAME_PROMPT.into(),
                    ));
                    continue;
                }
                out.extend(codes(page.page, text, &lower, ls, ps, pe));
            }
        }
    }
    settle(std::slice::from_ref(page), &mut out);
    out
}

/// Ask about each unplaced value once: a later detection with the same text
/// as an earlier question stays inked without a question of its own.
pub fn settle(pages: &[PageText], dets: &mut [Detection]) {
    let mut asked: Vec<String> = Vec::new();
    for d in dets.iter_mut().filter(|d| {
        d.needs_review
            && d.field.is_none()
            && matches!(&d.source, DetectionSource::FailClosed { reason } if reason.starts_with("unplaced_"))
    }) {
        let Some(v) = pages
            .iter()
            .find(|p| p.page == d.page)
            .and_then(|p| p.text.get(d.span.start..d.span.end))
        else {
            continue;
        };
        let v = v.split_whitespace().collect::<Vec<_>>().join(" ");
        if asked.contains(&v) {
            d.needs_review = false;
            d.review_prompt = None;
        } else {
            asked.push(v);
        }
    }
}

/// Column segments of a line: three spaces or a tab end a column.
fn segments(line: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut gap_start = None;
    let mut spaces = 0;
    for (i, c) in line.char_indices() {
        match c {
            ' ' => {
                gap_start.get_or_insert(i);
                spaces += 1;
            }
            '\t' => {
                gap_start.get_or_insert(i);
                spaces += 3;
            }
            _ => {
                if let Some(g) = gap_start.take()
                    && spaces >= 3
                {
                    if start < g {
                        out.push((start, g));
                    }
                    start = i;
                }
                spaces = 0;
            }
        }
    }
    if start < line.len() {
        out.push((start, line.len()));
    }
    out
}

/// `Person` or `Org` when `piece` is 2 to 4 capitalised words, at least two
/// of them longer than an initial.
fn name_kind(piece: &str) -> Option<TokenKind> {
    let words: Vec<&str> = piece.split_whitespace().collect();
    if !NAME_WORDS.contains(&words.len()) {
        return None;
    }
    let last = words.last()?.replace('.', "").to_ascii_lowercase();
    let org = ORG_ENDINGS.contains(&last.as_str());
    let mut full = 0;
    for (i, w) in words.iter().enumerate() {
        // An org ending may be in capitals ("LP").
        if is_initial(w) || (org && i + 1 == words.len()) {
            continue;
        }
        if !is_capitalised(w) {
            return None;
        }
        full += 1;
    }
    if full < 2 {
        return None;
    }
    Some(if org {
        TokenKind::Org
    } else {
        TokenKind::Person
    })
}

/// One capital letter, optionally with a dot.
fn is_initial(w: &str) -> bool {
    let w = w.strip_suffix('.').unwrap_or(w);
    w.len() == 1 && w.bytes().all(|b| b.is_ascii_uppercase())
}

/// A capital, then letters (at least one lowercase), hyphens or apostrophes.
fn is_capitalised(w: &str) -> bool {
    let mut chars = w.chars();
    chars.next().is_some_and(char::is_uppercase)
        && w.chars().any(char::is_lowercase)
        && chars.all(|c| c.is_alphabetic() || matches!(c, '-' | '\'' | '\u{2019}' | '.'))
}

/// Code- and number-shaped tokens in `[ps, pe)`; `ls` is the line start.
fn codes(page: u32, text: &str, lower: &str, ls: usize, ps: usize, pe: usize) -> Vec<Detection> {
    let mut out = Vec::new();
    for (ts, te) in shape::tokens(&text[ps..pe]) {
        let raw = &text[ps + ts..ps + te];
        let lead = raw.len() - raw.trim_start_matches(['(', '[']).len();
        let tok = raw[lead..].trim_end_matches([')', ']', ',', '.', ';', ':']);
        let (s, e) = (ps + ts + lead, ps + ts + lead + tok.len());
        if !is_code(tok) || fixed_number(&lower[ls..s]) {
            continue;
        }
        // A state code, then an ID: a state ID box ("OR 0000123-4").
        let word_before = text[ls..s]
            .trim_end()
            .rsplit(char::is_whitespace)
            .next()
            .unwrap_or_default();
        if address::STATES.contains(&word_before) {
            let source = DetectionSource::FailClosed {
                reason: "state_id".into(),
            };
            out.push(detection(
                page,
                s,
                e,
                TokenKind::Custom("state_id".into()),
                source,
                CONF,
                Treatment::Tokenize,
            ));
            continue;
        }
        if ACCOUNT_CODE.is_match(tok)
            && label_before(lower, s, ACCOUNT_WINDOW, ACCOUNT_LABELS).is_some()
        {
            let source = DetectionSource::FailClosed {
                reason: "account_code".into(),
            };
            out.push(detection(
                page,
                s,
                e,
                TokenKind::Account,
                source,
                CONF,
                Treatment::Tokenize,
            ));
            continue;
        }
        out.push(held(
            page,
            s,
            e,
            TokenKind::Custom("number".into()),
            "unplaced_number",
            CONF,
            NUMBER_PROMPT.into(),
        ));
    }
    out
}

/// True when `tok` is shaped like an ID: five or more characters mixing
/// letters and digits or joining digit groups with a dash, or six or more
/// digits in a row; never an amount, date, ZIP, form number or box code.
fn is_code(tok: &str) -> bool {
    if tok.len() < 5 || !CODE.is_match(tok) {
        return false;
    }
    let letters = tok.bytes().any(|b| b.is_ascii_alphabetic());
    let digits = tok.bytes().any(|b| b.is_ascii_digit());
    let shaped = (letters && digits) || DASHED_DIGITS.is_match(tok) || LONG_DIGITS.is_match(tok);
    shaped
        && !shape::is_benign_token(tok)
        && !ZIP.is_match(tok)
        && !FORM.is_match(tok)
        && !GLUED_YEAR.is_match(tok)
}

/// True when the line before a token ends with a label whose number is
/// fixed form text (`OMB No.`, `Cat. No.`).
fn fixed_number(before: &str) -> bool {
    let b = before.trim_end();
    FIXED_NUMBER_LABELS.iter().any(|l| b.ends_with(l))
}

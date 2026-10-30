//! One value, one kind. A return prints the taxpayer's SSN in the page 1 box
//! and again in the header of every later page and schedule, often as nine
//! bare digits with no label close enough for the rules to place it. Once a
//! number is placed with confidence as an SSN, ITIN, EIN, routing or account
//! number anywhere in the document, every other occurrence of the same
//! digits, with or without dashes, spaces or dots between groups, on every
//! page and in every form field, is detected with the same kind. A repeat is
//! then never a "number I can't place" that a review answer could leave in
//! the clear, and one value maps to one token everywhere.

use inker_core::{Detection, DetectionSource, PageText, TokenKind, Treatment};
use regex::Regex;

use crate::shape;
use crate::util::{detection, digits, re, rule};

/// Least confidence for a detection to vouch for its digits elsewhere. The
/// rules give a reserved-area SSN after an SSN label 0.45.
pub const MIN_CONF: f32 = 0.45;
/// Fewest digits in a value worth repeating (the shortest account number).
const MIN_DIGITS: usize = 6;
/// What may sit between two digits of a repeat: one dash (any dash variant),
/// space or dot.
const SEP: &str = r"[-. \x{2010}-\x{2015}\x{2212}\x{fe63}\x{ff0d}]?";

/// Kinds whose value is a number that identifies someone.
fn repeats(kind: &TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Ssn | TokenKind::Itin | TokenKind::Ein | TokenKind::Routing | TokenKind::Account
    )
}

/// A placed number: its digits, kind and confidence.
struct Anchor {
    digits: String,
    kind: TokenKind,
    confidence: f32,
    pattern: Regex,
}

/// The text a detection's span indexes into: its page, or its form field.
fn text_of<'t>(
    pages: &'t [PageText],
    fields: &'t [(&str, PageText)],
    d: &Detection,
) -> Option<&'t str> {
    match &d.field {
        Some(name) => fields
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, p)| p.text.as_str()),
        None => pages
            .iter()
            .find(|p| p.page == d.page)
            .map(|p| p.text.as_str()),
    }
}

/// The placed numbers in `dets`, one per digit string (the most confident).
fn anchors(pages: &[PageText], fields: &[(&str, PageText)], dets: &[Detection]) -> Vec<Anchor> {
    let mut out: Vec<Anchor> = Vec::new();
    for d in dets {
        if !repeats(&d.kind)
            || d.needs_review
            || d.treatment != Treatment::Tokenize
            || d.confidence < MIN_CONF
            || matches!(d.source, DetectionSource::FailClosed { .. })
        {
            continue;
        }
        let Some(v) = text_of(pages, fields, d).and_then(|t| t.get(d.span.start..d.span.end))
        else {
            continue;
        };
        // Digits and separators only: a code with letters is not repeated.
        if v.chars().any(char::is_alphabetic) {
            continue;
        }
        let ds = digits(v);
        if ds.len() < MIN_DIGITS {
            continue;
        }
        match out.iter_mut().find(|a| a.digits == ds) {
            Some(a) if a.confidence < d.confidence => {
                a.kind = d.kind.clone();
                a.confidence = d.confidence;
            }
            Some(_) => {}
            None => {
                let body: Vec<String> = ds.chars().map(String::from).collect();
                out.push(Anchor {
                    pattern: re(&body.join(SEP)),
                    digits: ds,
                    kind: d.kind.clone(),
                    confidence: d.confidence,
                });
            }
        }
    }
    out
}

/// True when a digit or a digit joined by a separator sits next to
/// `[start, end)`, so the match is part of a longer number.
fn glued(text: &str, start: usize, end: usize) -> bool {
    let b = text.as_bytes();
    let sep = |c: u8| matches!(c, b'-' | b'.' | b',' | b'/');
    let before = match start.checked_sub(1).map(|i| b[i]) {
        Some(c) if c.is_ascii_digit() => true,
        Some(c) if sep(c) => start >= 2 && b[start - 2].is_ascii_digit(),
        _ => false,
    };
    let after = match b.get(end) {
        Some(c) if c.is_ascii_digit() => true,
        Some(&c) if sep(c) => b.get(end + 1).is_some_and(u8::is_ascii_digit),
        _ => false,
    };
    before || after
}

/// Every occurrence of an anchor's digits in `text`, as detections on
/// `page` (and `field`).
fn repeats_in(
    anchors: &[Anchor],
    page: u32,
    field: Option<&str>,
    text: &str,
    out: &mut Vec<Detection>,
) {
    for a in anchors {
        for m in a.pattern.find_iter(text) {
            let v = m.as_str();
            // "1000.00" has an account's digits but is an amount.
            let benign = !v.contains(' ') && shape::is_benign_token(v);
            if benign || glued(text, m.start(), m.end()) {
                continue;
            }
            let mut d = detection(
                page,
                m.start(),
                m.end(),
                a.kind.clone(),
                rule("same_value"),
                a.confidence,
                Treatment::Tokenize,
            );
            d.field = field.map(str::to_string);
            out.push(d);
        }
    }
}

/// Detections for every repeat of a placed number in `pages` and `fields`.
/// Merge them with `dets`: a repeat outranks a fail-closed hold over the
/// same digits and gives way to a template or rule detection already there.
pub fn find(pages: &[PageText], fields: &[(&str, PageText)], dets: &[Detection]) -> Vec<Detection> {
    let anchors = anchors(pages, fields, dets);
    let mut out = Vec::new();
    if anchors.is_empty() {
        return out;
    }
    for p in pages {
        repeats_in(&anchors, p.page, None, &p.text, &mut out);
    }
    for (name, p) in fields {
        repeats_in(&anchors, p.page, Some(name), &p.text, &mut out);
    }
    tracing::debug!(anchors = anchors.len(), repeats = out.len(), "same value");
    out
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use inker_core::TextOrigin;

    fn page(n: u32, text: &str) -> PageText {
        PageText {
            page: n,
            text: text.into(),
            origin: TextOrigin::TextLayer,
        }
    }

    fn placed(page: u32, text: &str, v: &str, kind: TokenKind, conf: f32) -> Detection {
        let s = text.find(v).unwrap();
        detection(
            page,
            s,
            s + v.len(),
            kind,
            rule("t"),
            conf,
            Treatment::Tokenize,
        )
    }

    fn values<'a>(pages: &'a [PageText], dets: &[Detection]) -> Vec<&'a str> {
        dets.iter()
            .map(|d| &pages[d.page as usize - 1].text[d.span.start..d.span.end])
            .collect()
    }

    #[test]
    fn repeats_with_any_grouping_take_the_kind() {
        let p1 = "SSN 900-12-3456";
        let pages = [
            page(1, p1),
            page(
                2,
                "Jordan A Example 900123456\nalso 900 12 3456 and 900.12.3456",
            ),
        ];
        let dets = [placed(1, p1, "900-12-3456", TokenKind::Ssn, 0.55)];
        let got = find(&pages, &[], &dets);
        assert_eq!(
            values(&pages, &got),
            ["900-12-3456", "900123456", "900 12 3456", "900.12.3456"]
        );
        assert!(
            got.iter()
                .all(|d| d.kind == TokenKind::Ssn && !d.needs_review)
        );
    }

    #[test]
    fn a_longer_number_or_an_amount_is_not_a_repeat() {
        let p1 = "Account 1000001";
        let pages = [
            page(1, p1),
            page(2, "ref 81000001 then 10000.01 and 1000001-2"),
        ];
        let dets = [placed(1, p1, "1000001", TokenKind::Account, 0.85)];
        let got = find(&pages, &[], &dets);
        assert_eq!(values(&pages, &got), ["1000001"]);
    }

    #[test]
    fn held_or_unsure_values_vouch_for_nothing() {
        let p1 = "x 900123456 y 900123457";
        let pages = [page(1, p1), page(2, "900123456 900123457")];
        let mut held = placed(1, p1, "900123456", TokenKind::Ssn, 0.9);
        held.needs_review = true;
        let weak = placed(1, p1, "900123457", TokenKind::Ssn, 0.3);
        assert!(find(&pages, &[], &[held, weak]).is_empty());
    }

    #[test]
    fn form_fields_get_repeats_too() {
        let p1 = "EIN 07-1234567";
        let pages = [page(1, p1)];
        let fields = [("f.ein", page(1, "071234567"))];
        let dets = [placed(1, p1, "07-1234567", TokenKind::Ein, 0.5)];
        let got = find(&pages, &fields, &dets);
        let f = got.iter().find(|d| d.field.is_some()).unwrap();
        assert_eq!(
            (f.kind.clone(), f.span.start, f.span.end),
            (TokenKind::Ein, 0, 9)
        );
    }
}

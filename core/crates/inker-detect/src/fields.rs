//! Form field values. Each value is scanned as a page of its own by the same
//! detectors as page text; whatever they leave that is not an amount, a date,
//! a code or a checkbox mark is held for review, never passed through.

use inker_core::{Detection, FormField, PageText, TextOrigin, TokenKind};

use crate::failclosed::held;
use crate::shape;
use crate::util::{overlaps, uncovered};

/// Fail-closed confidence for a value no detector could place.
const CONF: f32 = 0.3;
/// Digits that make a leftover look like a number rather than words.
const NUMBER_DIGITS: usize = 6;

/// A field value as a pseudo-page, so the page detectors can run over it.
pub fn page(field: &FormField) -> PageText {
    PageText {
        page: field.page.unwrap_or(1),
        text: field.value.clone(),
        origin: TextOrigin::TextLayer,
    }
}

/// Point detections found on a field's pseudo-page at that field.
pub fn tag(found: impl IntoIterator<Item = Detection>, name: &str) -> Vec<Detection> {
    found
        .into_iter()
        .map(|mut d| {
            d.field = Some(name.to_string());
            d
        })
        .collect()
}

/// Hold every stretch of a field value that `covered` misses and that is not
/// a benign token. Adjacent stretches (only whitespace between) are held as one.
pub fn hold_rest(page: &PageText, covered: &[(usize, usize)]) -> Vec<Detection> {
    let text = page.text.as_str();
    let mut spans: Vec<(usize, usize)> = Vec::new();
    for (ts, te) in shape::tokens(text) {
        for (s, e) in uncovered(ts, te, covered) {
            let piece = &text[s..e];
            if !piece.chars().any(char::is_alphanumeric) || shape::is_benign_token(piece) {
                continue;
            }
            match spans.last_mut() {
                Some(last)
                    if text[last.1..s].trim().is_empty() && !overlaps(covered, last.1, s) =>
                {
                    last.1 = e;
                }
                _ => spans.push((s, e)),
            }
        }
    }
    spans
        .into_iter()
        .map(|(s, e)| {
            let digits = text[s..e].chars().filter(char::is_ascii_digit).count();
            let (kind, prompt) = if digits >= NUMBER_DIGITS {
                (
                    TokenKind::Custom("number".into()),
                    "A long number in a form field I can't place. Ink it?",
                )
            } else {
                (
                    TokenKind::Custom("field".into()),
                    "A form field value I can't place. Ink it?",
                )
            };
            held(page.page, s, e, kind, "unplaced_field", CONF, prompt.into())
        })
        .collect()
}

//! A multi-page return where the taxpayer's SSN is printed as nine bare
//! digits in the page 1 box, in the header of later pages and schedules, and
//! once spaced or dashed; and a date of birth after a birth-date label.
//! Every occurrence must be inked as an SSN, and the date of birth inked;
//! other dates on the return stay in the clear.
//! Fixture `1040-undashed-ssn.txt`: synthetic (SSN area 900), pages split by
//! a form feed.

#![allow(clippy::unwrap_used, clippy::expect_used)] // tests may panic

use std::path::PathBuf;

use inker_core::{Detection, Extracted, PageText, TextOrigin, TokenKind};
use inker_detect::detect;
use inker_packs::PackSet;

fn packs() -> PackSet {
    PackSet::load(Some(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../packs/tax"),
    ))
    .unwrap()
}

fn extracted(origin: TextOrigin) -> Extracted {
    let text = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/1040-undashed-ssn.txt"),
    )
    .unwrap();
    Extracted {
        pages: text
            .split('\u{c}')
            .enumerate()
            .map(|(i, t)| PageText {
                page: i as u32 + 1,
                text: t.into(),
                origin,
            })
            .collect(),
        fields: vec![],
    }
}

/// Every occurrence of `v` on every page, as (page, start, end).
fn occurrences(e: &Extracted, v: &str) -> Vec<(u32, usize, usize)> {
    e.pages
        .iter()
        .flat_map(|p| {
            p.text
                .match_indices(v)
                .map(move |(i, m)| (p.page, i, i + m.len()))
        })
        .collect()
}

/// The detection wholly covering `[s, e)` on `page`, if any.
fn covering(dets: &[Detection], page: u32, s: usize, e: usize) -> Option<&Detection> {
    dets.iter()
        .find(|d| d.page == page && d.field.is_none() && d.span.start <= s && e <= d.span.end)
}

fn touched(dets: &[Detection], page: u32, s: usize, e: usize) -> bool {
    dets.iter()
        .any(|d| d.page == page && d.field.is_none() && d.span.start < e && s < d.span.end)
}

const SSN_FORMS: &[&str] = &["900123456", "900 12 3456", "900-12-3456"];

fn every_ssn_is_inked_as_ssn(origin: TextOrigin) {
    let e = extracted(origin);
    let (dets, _) = detect(&e, &packs()).unwrap();
    let mut n = 0;
    for v in SSN_FORMS {
        for (page, s, end) in occurrences(&e, v) {
            n += 1;
            let d = covering(&dets, page, s, end)
                .unwrap_or_else(|| panic!("SSN form {v:?} on page {page} at {s} is not inked"));
            assert_eq!(d.kind, TokenKind::Ssn, "{v:?} on page {page} at {s}");
            // A held repeat is a question a "Leave it" answer turns into a
            // value in the clear.
            assert!(!d.needs_review, "{v:?} on page {page} at {s} is held");
        }
    }
    assert_eq!(n, 7, "fixture has seven SSN occurrences");
}

#[test]
fn bare_ssn_is_inked_everywhere_on_a_text_layer_return() {
    every_ssn_is_inked_as_ssn(TextOrigin::TextLayer);
}

#[test]
fn bare_ssn_is_inked_everywhere_on_a_scanned_return() {
    every_ssn_is_inked_as_ssn(TextOrigin::Ocr);
}

#[test]
fn date_of_birth_is_inked_after_a_birth_label() {
    let e = extracted(TextOrigin::TextLayer);
    let (dets, _) = detect(&e, &packs()).unwrap();
    let dob = TokenKind::Custom("dob".into());
    let mut n = 0;
    for v in ["03/14/1985", "March 14, 1985"] {
        for (page, s, end) in occurrences(&e, v) {
            n += 1;
            let d = covering(&dets, page, s, end)
                .unwrap_or_else(|| panic!("DOB {v:?} on page {page} is not inked"));
            assert_eq!(d.kind, dob, "{v:?} on page {page}");
        }
    }
    assert_eq!(n, 4, "fixture has four dates of birth");
}

#[test]
fn other_dates_on_the_return_stay_clear() {
    let e = extracted(TextOrigin::TextLayer);
    let (dets, _) = detect(&e, &packs()).unwrap();
    for (page, s, end) in occurrences(&e, "04/10/2024") {
        assert!(
            !touched(&dets, page, s, end),
            "the signature date on page {page} was inked"
        );
    }
    let (page, s, end) = occurrences(&e, "2023")[0];
    assert!(!touched(&dets, page, s, end), "the tax year was inked");
}

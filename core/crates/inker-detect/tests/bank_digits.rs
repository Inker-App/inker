//! Direct-deposit routing and account numbers in every shape a Form 1040's
//! digit boxes come out of a text layer or OCR. Each test runs the whole
//! detector, then checks that every digit of each number sits inside a
//! routing or account detection, and that tokenizing leaves none of it raw.
//!
//! Values are synthetic: 123456789 and 987654321 fail the ABA checksum, and
//! the account numbers are made up.

#![allow(clippy::unwrap_used, clippy::expect_used)] // tests may panic

use std::path::PathBuf;

use camino::Utf8PathBuf;
use inker_core::tokenize::tokenize;
use inker_core::vault::{MemoryKeyStore, Vault};
use inker_core::{Detection, Extracted, FormField, PageText, TextOrigin, TokenKind, Treatment};
use inker_detect::detect;
use inker_packs::PackSet;
use regex::Regex;

const ROUTING: &str = "123456789";
const ACCOUNT: &str = "000123456789";

fn packs() -> PackSet {
    PackSet::load(Some(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../packs/tax"),
    ))
    .unwrap()
}

/// The synthetic 1040 fixture with its direct-deposit block (lines 35b to
/// 35d) replaced by `block`.
fn form_1040(block: &str) -> String {
    let base = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/1040.txt"),
    )
    .unwrap();
    let start = base.find("b Routing number").unwrap();
    let end = base.find("36 Amount of line 34").unwrap();
    format!("{}{}\n{}", &base[..start], block.trim_end(), &base[end..])
}

fn page(text: &str, origin: TextOrigin) -> Extracted {
    Extracted {
        pages: vec![PageText {
            page: 1,
            text: text.into(),
            origin,
        }],
        fields: vec![],
    }
}

/// Every place `value`'s digits appear in `text`, one digit per box or with
/// spaces or dashes between them, as the byte offsets of its digits.
fn occurrences(text: &str, value: &str) -> Vec<Vec<usize>> {
    let pattern: Vec<String> = value.chars().map(|c| c.to_string()).collect();
    let re = Regex::new(&format!(
        r"(?:^|[^0-9]){}(?:[^0-9]|$)",
        pattern.join("[ -]*")
    ))
    .unwrap();
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(m) = re.find_at(text, from) {
        let digits: Vec<usize> = (m.start()..m.end())
            .filter(|&i| text.as_bytes()[i].is_ascii_digit())
            .collect();
        from = *digits.last().unwrap();
        out.push(digits);
    }
    out
}

/// Every digit of every occurrence of `value` on the page is inside a
/// tokenized routing or account detection (in `field`, any tokenized one).
fn assert_covered(text: &str, dets: &[Detection], field: Option<&str>, value: &str) {
    let found = occurrences(text, value);
    assert!(!found.is_empty(), "fixture does not contain the value");
    for digits in found {
        for i in digits {
            let hit = dets
                .iter()
                .find(|d| d.field.as_deref() == field && d.span.start <= i && i < d.span.end);
            match hit {
                // A form field's value has no label beside it: any token
                // will do there. On the page it must read as a bank number.
                Some(d)
                    if (field.is_some()
                        || matches!(d.kind, TokenKind::Routing | TokenKind::Account))
                        && d.treatment == Treatment::Tokenize => {}
                other => panic!(
                    "digit at byte {i} of a bank number is {}; detections: {:?}",
                    other.map_or("raw".to_string(), |d| format!("{:?}", d.kind)),
                    dets.iter()
                        .filter(|d| d.field.as_deref() == field
                            && d.span.end + 200 > i
                            && d.span.start < i + 200)
                        .map(|d| (
                            d.kind.clone(),
                            &text[d.span.start..d.span.end],
                            d.source.clone()
                        ))
                        .collect::<Vec<_>>()
                ),
            }
        }
    }
}

/// Tokenize with a throwaway vault: no digit of either number survives,
/// and no line with a bank token keeps a long run of digits.
fn assert_inked_output(e: &Extracted, dets: &[Detection]) {
    let dir = tempfile::tempdir().unwrap();
    let p = Utf8PathBuf::from_path_buf(dir.path().join("vault.bin")).unwrap();
    let vault = Vault::open(&p, &MemoryKeyStore::new().unwrap()).unwrap();
    let out = tokenize(&vault, e, dets).unwrap();
    let texts = out
        .pages
        .iter()
        .map(|p| p.text.as_str())
        .chain(out.fields.iter().map(|f| f.value.as_str()));
    for t in texts {
        for v in [ROUTING, ACCOUNT] {
            assert!(
                occurrences(t, v).is_empty(),
                "a bank number is still raw in the output"
            );
        }
        for l in t
            .lines()
            .filter(|l| l.contains("[ACCOUNT_") || l.contains("[ROUTING_"))
        {
            let digits = l
                .replace("[ACCOUNT_", "")
                .replace("[ROUTING_", "")
                .chars()
                .filter(char::is_ascii_digit)
                .count();
            assert!(digits < 6, "raw digits beside a bank token: {digits}");
        }
    }
}

fn check_page(block: &str, origin: TextOrigin) {
    let text = form_1040(block);
    let e = page(&text, origin);
    let (dets, _) = detect(&e, &packs()).unwrap();
    assert_covered(&text, &dets, None, ROUTING);
    assert_covered(&text, &dets, None, ACCOUNT);
    assert_inked_output(&e, &dets);
}

const SPACED: &str = "b Routing number    1 2 3 4 5 6 7 8 9           c Type:   [X] Checking   [ ] Savings\n\
                      d Account number    0 0 0 1 2 3 4 5 6 7 8 9";

// (a) One digit per box, separated by single spaces.
#[test]
fn spaced_digit_boxes_text_layer() {
    check_page(SPACED, TextOrigin::TextLayer);
}

#[test]
fn spaced_digit_boxes_ocr() {
    check_page(SPACED, TextOrigin::Ocr);
}

#[test]
fn spaced_digit_boxes_without_a_template() {
    let text = "Direct deposit\n\
                Routing number    1 2 3 4 5 6 7 8 9\n\
                Account number    0 0 0 1 2 3 4 5 6 7 8 9\n";
    let e = page(text, TextOrigin::TextLayer);
    let (dets, _) = detect(&e, &packs()).unwrap();
    assert_covered(text, &dets, None, ROUTING);
    assert_covered(text, &dets, None, ACCOUNT);
    assert_inked_output(&e, &dets);
}

// (b) The labels on one line, both values together on the next.
#[test]
fn labels_on_one_line_values_on_the_next() {
    check_page(
        "b Routing number       c Type:   [X] Checking   [ ] Savings     d Account number\n\
         123456789 000123456789",
        TextOrigin::TextLayer,
    );
}

#[test]
fn labels_on_one_line_spaced_values_on_the_next() {
    check_page(
        "b Routing number       c Type:   [X] Checking   [ ] Savings     d Account number\n\
         1 2 3 4 5 6 7 8 9    0 0 0 1 2 3 4 5 6 7 8 9",
        TextOrigin::TextLayer,
    );
}

// (c) The account number as a form-field value and in the page text.
#[test]
fn account_in_a_field_and_in_the_page_text() {
    let text = form_1040(
        "b Routing number    123456789           c Type:   [X] Checking   [ ] Savings\n\
         d Account number    000123456789",
    );
    let routing_field = "topmostSubform[0].Page2[0].f2_28[0]";
    let account_field = "topmostSubform[0].Page2[0].f2_29[0]";
    let mut e = page(&text, TextOrigin::TextLayer);
    e.fields = vec![
        FormField {
            name: routing_field.into(),
            value: ROUTING.into(),
            page: Some(1),
        },
        FormField {
            name: account_field.into(),
            value: ACCOUNT.into(),
            page: Some(1),
        },
    ];
    let (dets, _) = detect(&e, &packs()).unwrap();
    assert_covered(&text, &dets, None, ROUTING);
    assert_covered(&text, &dets, None, ACCOUNT);
    assert_covered(ROUTING, &dets, Some(routing_field), ROUTING);
    assert_covered(ACCOUNT, &dets, Some(account_field), ACCOUNT);
    assert_inked_output(&e, &dets);
}

#[test]
fn spaced_account_in_a_field_and_in_the_page_text() {
    let text = form_1040(SPACED);
    let account_field = "topmostSubform[0].Page2[0].f2_29[0]";
    let mut e = page(&text, TextOrigin::TextLayer);
    e.fields = vec![FormField {
        name: account_field.into(),
        value: ACCOUNT.into(),
        page: Some(1),
    }];
    let (dets, _) = detect(&e, &packs()).unwrap();
    assert_covered(&text, &dets, None, ACCOUNT);
    assert_covered(ACCOUNT, &dets, Some(account_field), ACCOUNT);
    assert_inked_output(&e, &dets);
}

// (d) The template's field value covers only part of the digit run.
#[test]
fn template_value_covers_part_of_a_grouped_account() {
    check_page(
        "b Routing number    123456789           c Type:   [X] Checking   [ ] Savings\n\
         d Account number    0001 2345 6789",
        TextOrigin::TextLayer,
    );
}

#[test]
fn template_value_covers_part_of_a_split_run() {
    check_page(
        "b Routing number    1234 56789           c Type:   [X] Checking   [ ] Savings\n\
         d Account number    00012 3456789",
        TextOrigin::TextLayer,
    );
}

// (e) Routing then account on one line after "Routing number", the account
// label far away.
#[test]
fn routing_and_account_on_one_line_after_the_routing_label() {
    check_page(
        "b Routing number    123456789   000123456789\n\
         c Type:   [X] Checking   [ ] Savings\n\
         Direct deposit details continue on the next line of the return as printed\n\
         d Account number",
        TextOrigin::TextLayer,
    );
}

#[test]
fn routing_and_account_on_one_line_single_space() {
    check_page(
        "b Routing number 123456789 000123456789          c Type:   [X] Checking   [ ] Savings\n\
         d Account number",
        TextOrigin::TextLayer,
    );
}

// Amounts around the block are not bank numbers.
#[test]
fn amounts_are_not_inked() {
    let text = form_1040(SPACED);
    let e = page(&text, TextOrigin::TextLayer);
    let (dets, _) = detect(&e, &packs()).unwrap();
    for amount in ["2,670.40", "11,912.40"] {
        let at = text.find(amount).unwrap();
        assert!(
            !dets
                .iter()
                .any(|d| d.field.is_none() && d.span.start < at + amount.len() && at < d.span.end),
            "an amount was inked"
        );
    }
}

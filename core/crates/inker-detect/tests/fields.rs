//! Form fields with no template mapping: every value is scanned, and what the
//! detectors cannot place is held rather than passed through (finding 1).
//!
//! Values are synthetic: SSN areas 666/9xx, EIN prefixes 07/89, invented names.

#![allow(clippy::unwrap_used, clippy::expect_used)] // tests may panic

use std::path::PathBuf;

use camino::Utf8Path;
use inker_core::{Detection, Extracted, FormField, PageText, TextOrigin, ports::TextSource};
use inker_detect::detect;
use inker_extract::PdfTextSource;
use inker_packs::PackSet;

fn packs() -> PackSet {
    PackSet::load(Some(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../packs/tax"),
    ))
    .unwrap()
}

fn field(name: &str, value: &str) -> FormField {
    FormField {
        name: name.into(),
        value: value.into(),
        page: Some(1),
    }
}

/// The parts of a field value no detection covers, with whitespace and
/// punctuation dropped: what an agent would read in the clear.
fn visible(value: &str, name: &str, dets: &[Detection]) -> String {
    value
        .char_indices()
        .filter(|(i, _)| {
            !dets
                .iter()
                .any(|d| d.field.as_deref() == Some(name) && d.span.start <= *i && *i < d.span.end)
        })
        .map(|(_, c)| c)
        .filter(|c| c.is_alphanumeric())
        .collect()
}

fn of<'a>(dets: &'a [Detection], name: &str) -> Vec<&'a Detection> {
    dets.iter()
        .filter(|d| d.field.as_deref() == Some(name))
        .collect()
}

#[test]
fn unmapped_fields_are_scanned_and_held() {
    let e = Extracted {
        pages: vec![PageText {
            page: 1,
            text: "Request for Taxpayer Identification Number and Certification".into(),
            origin: TextOrigin::TextLayer,
        }],
        fields: vec![
            field("topmostSubform[0].Page1[0].f1_11[0]", "666-12-3456"),
            field("f1_01", "Riley Sample"),
            field("f1_02", "07-4400123"),
            field("f1_03", "4471002233"),
            field("f1_04", "ref 551203"),
            field("f1_05", "1428 Elm Street Apt 4B, Brooklyn, NY 11201"),
            field("f1_06", "lowercase words here"),
        ],
    };
    let (dets, _) = detect(&e, &packs()).unwrap();
    for f in &e.fields {
        let mine = of(&dets, &f.name);
        assert!(!mine.is_empty(), "{} passed through", f.name);
        assert_eq!(visible(&f.value, &f.name, &dets), "", "{} leaks", f.name);
        for d in mine {
            assert!(d.span.end <= f.value.len());
        }
    }
    // A value no detector classifies is held for review.
    for name in ["f1_03", "f1_04", "f1_06"] {
        assert!(
            of(&dets, name).iter().any(|d| d.needs_review),
            "{name} not held"
        );
    }
}

#[test]
fn benign_field_values_stay_clear() {
    let e = Extracted {
        pages: vec![],
        fields: vec![
            field("a", "96,400.00"),
            field("b", "$1,204.55"),
            field("c", "(150.00)"),
            field("d", "-150.00"),
            field("e", "03/14/2025"),
            field("f", "2025-03-14"),
            field("g", "CO"),
            field("h", ""),
            field("i", "   "),
            field("j", "Off"),
            field("k", "X"),
            field("l", "12.00"),
        ],
    };
    let (dets, _) = detect(&e, &packs()).unwrap();
    assert!(dets.is_empty(), "{dets:?}");
}

#[test]
fn six_digit_runs_are_not_amounts() {
    let e = Extracted {
        pages: vec![],
        fields: vec![field("a", "123456"), field("b", "4471002233119.00")],
    };
    let (dets, _) = detect(&e, &packs()).unwrap();
    assert!(of(&dets, "a").iter().any(|d| d.needs_review));
    assert!(of(&dets, "b").iter().any(|d| d.needs_review));
}

#[test]
fn fillable_w2_fields_are_all_inked() {
    let path = Utf8Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../samples/synthetic/w2-bluefin-2025-fillable.pdf");
    let e = PdfTextSource::new().extract(&path).unwrap().unwrap();
    assert_eq!(e.fields.len(), 8);
    let (dets, class) = detect(&e, &packs()).unwrap();
    for f in &e.fields {
        let left = visible(&f.value, &f.name, &dets);
        match f.name.as_str() {
            // Amounts and the state code stay readable.
            "w2.box_1_wages" | "w2.box_2_fed_withheld" | "w2.box_15_state" => {
                assert_eq!(left, f.value.replace([',', '.'], ""), "{}", f.name)
            }
            _ => assert_eq!(left, "", "{} leaks", f.name),
        }
    }
    assert_eq!(class.kind.as_str(), "w2");
}

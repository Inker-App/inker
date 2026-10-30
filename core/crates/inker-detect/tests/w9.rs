//! Form W-9: a filled first page, then pages of printed IRS instructions.
//!
//! The sample is samples/synthetic/w9-bluefin-2025 (see samples/generate.py):
//! Riley Sample, the owner of Bluefin Analytics LLC, an SSN in area 666, and
//! the synthetic requester Tidewater Partners LP. The instructions name the
//! IRS, the SSA and LLCs throughout and print public agency phone numbers and
//! email addresses. None of that is a typed value, so none of it is inked.

#![allow(clippy::unwrap_used, clippy::expect_used)] // tests may panic

use std::path::PathBuf;

use camino::Utf8PathBuf;
use inker_core::ports::TextSource;
use inker_core::{Detection, Extracted, PageText, TextOrigin, TokenKind};
use inker_detect::detect;
use inker_extract::PdfTextSource;
use inker_packs::PackSet;

fn packs() -> PackSet {
    PackSet::load(Some(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../packs/tax"),
    ))
    .unwrap()
}

fn sample(ext: &str) -> Utf8PathBuf {
    Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../samples/synthetic")
        .join(format!("w9-bluefin-2025.{ext}"))
}

/// The W-9's text pages: the .txt puts a form feed between them.
fn w9() -> Extracted {
    let text = std::fs::read_to_string(sample("txt")).unwrap();
    Extracted {
        pages: text
            .split('\u{c}')
            .zip(1..)
            .map(|(t, page)| PageText {
                page,
                text: t.into(),
                origin: TextOrigin::TextLayer,
            })
            .collect(),
        fields: vec![],
    }
}

fn value<'a>(e: &'a Extracted, d: &Detection) -> &'a str {
    let page = e.pages.iter().find(|p| p.page == d.page).unwrap();
    &page.text[d.span.start..d.span.end]
}

fn listing(e: &Extracted, dets: &[Detection]) -> Vec<String> {
    dets.iter()
        .map(|d| format!("p{} {:?} {:?} {:?}", d.page, d.kind, value(e, d), d.source))
        .collect()
}

/// The values typed on the form, each inked as the right kind.
fn assert_typed_values_found(e: &Extracted, dets: &[Detection]) {
    let have = |v: &str, kind: TokenKind| {
        dets.iter()
            .any(|d| d.page == 1 && d.kind == kind && value(e, d).contains(v))
    };
    for (v, kind) in [
        ("Riley Sample", TokenKind::Person),
        ("Bluefin Analytics LLC", TokenKind::Org),
        ("1720 Quarry Road", TokenKind::Address),
        ("666-48-2210", TokenKind::Ssn),
        ("Tidewater Partners LP", TokenKind::Org),
    ] {
        assert!(
            have(v, kind.clone()),
            "{v} as {kind:?}; have {:#?}",
            listing(e, dets)
        );
    }
}

/// What is printed by the IRS is never inked.
fn assert_print_left_alone(e: &Extracted, dets: &[Detection]) {
    let all = listing(e, dets);
    for d in dets {
        let v = value(e, d);
        if d.kind == TokenKind::Person {
            for agency in ["IRS", "Internal Revenue Service", "SSA", "Treasury"] {
                assert!(!v.contains(agency), "{v:?} is not a person: {all:#?}");
            }
        }
        if d.kind == TokenKind::Org {
            // An org needs a name before its suffix ("An LLC" has none).
            let named = v.split_whitespace().any(|w| {
                !matches!(
                    w.trim_matches(|c: char| !c.is_ascii_alphanumeric()),
                    "LLC" | "LP" | "Inc" | "A" | "An" | "The" | "a" | "an" | "the"
                )
            });
            assert!(named, "a bare {v:?} is not an org: {all:#?}");
        }
        assert!(
            !(d.kind == TokenKind::Phone && d.page > 1),
            "agency hotline {v:?} is not a phone to ink: {all:#?}"
        );
    }
    // The instruction pages carry no typed value at all.
    let on_instructions: Vec<&Detection> = dets.iter().filter(|d| d.page > 1).collect();
    assert!(
        on_instructions.is_empty(),
        "inked on the instructions: {:#?}",
        listing(e, dets)
            .into_iter()
            .filter(|l| !l.starts_with("p1 "))
            .collect::<Vec<_>>()
    );
}

/// At most: name, business name, street, city line, SSN, requester name,
/// requester street, requester city line, the typed signature (the same
/// name again) and the signing date. Ten, all on page 1.
const MAX_DETECTIONS: usize = 10;

#[test]
fn w9_inks_the_typed_values_and_nothing_printed() {
    let e = w9();
    assert_eq!(e.pages.len(), 3);
    let (dets, _) = detect(&e, &packs()).unwrap();
    assert_typed_values_found(&e, &dets);
    assert_print_left_alone(&e, &dets);
    assert!(
        dets.len() <= MAX_DETECTIONS,
        "{} detections: {:#?}",
        dets.len(),
        listing(&e, &dets)
    );
}

#[test]
fn w9_pdf_inks_the_typed_values_and_nothing_printed() {
    let e = PdfTextSource::new()
        .extract(&sample("pdf"))
        .unwrap()
        .unwrap();
    assert_eq!(e.pages.len(), 3);
    let (dets, _) = detect(&e, &packs()).unwrap();
    assert_typed_values_found(&e, &dets);
    assert_print_left_alone(&e, &dets);
    assert!(dets.len() <= MAX_DETECTIONS, "{:#?}", listing(&e, &dets));
}

#[test]
fn w9_is_classified_as_a_w9() {
    let (_, class) = detect(&w9(), &packs()).unwrap();
    assert_eq!(class.kind.as_str(), "w9");
}

#[test]
fn instruction_pages_hold_no_names_but_keep_the_identifier_rules() {
    // A page under the W-9's running head is printed instructions: names,
    // organisations and addresses there are examples, not typed values. An
    // SSN-shaped number is still inked: the precise rules run everywhere.
    let mut e = w9();
    e.pages.truncate(1);
    e.pages.push(PageText {
        page: 2,
        text: "Form W-9 (Rev. 3-2024)   Page 4\n\
               Name: Pat Example\n\
               Example. Coho Partners LLC pays Pat Example.\n\
               48 Lantern Way Apt 3\n\
               Springfield, OR 97477\n\
               SSN 900-12-3456\n"
            .into(),
        origin: TextOrigin::TextLayer,
    });
    let (dets, _) = detect(&e, &packs()).unwrap();
    let page2: Vec<&Detection> = dets.iter().filter(|d| d.page == 2).collect();
    assert_eq!(page2.len(), 1, "{:#?}", listing(&e, &dets));
    assert_eq!(page2[0].kind, TokenKind::Ssn);
    // The same page with no W-9 before it is not an instruction page.
    let alone = Extracted {
        pages: vec![PageText {
            page: 1,
            ..e.pages[1].clone()
        }],
        fields: vec![],
    };
    let (dets, _) = detect(&alone, &packs()).unwrap();
    assert!(
        dets.iter().any(|d| d.kind == TokenKind::Person),
        "{:#?}",
        listing(&alone, &dets)
    );
}

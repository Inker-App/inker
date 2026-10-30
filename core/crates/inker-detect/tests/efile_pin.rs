//! Form 8879, IRS e-file Signature Authorization, and signature PINs.
//!
//! A user's 8879 came back from an agent with nothing inked: its names,
//! SSNs and signature PINs all went out raw. These run the full Detector
//! over the synthetic 8879 in samples/synthetic/ (both renditions) and over
//! PINs on pages no template matches.
//!
//! Values are synthetic: SSN areas 900 and 666, invented names, made-up
//! five-digit PINs and an all-zero EFIN.

#![allow(clippy::unwrap_used, clippy::expect_used)] // tests may panic

use std::path::PathBuf;

use inker_core::{Detection, DetectionSource, Extracted, PageText, TextOrigin, TokenKind};
use inker_detect::{Classification, detect};
use inker_packs::PackSet;

fn packs() -> PackSet {
    PackSet::load(Some(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../packs/tax"),
    ))
    .unwrap()
}

fn sample(name: &str) -> String {
    std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../samples/synthetic")
            .join(name),
    )
    .unwrap()
}

/// The 8879 as a text layer.
fn text_8879() -> Extracted {
    Extracted {
        pages: vec![PageText {
            page: 1,
            text: sample("8879-example-2023.txt"),
            origin: TextOrigin::TextLayer,
        }],
        fields: vec![],
    }
}

/// The 8879 as OCR of a scan.
fn ocr_8879() -> Extracted {
    let v: serde_json::Value = serde_json::from_str(&sample("8879-example-2023.ocr.json")).unwrap();
    let pages = v["pages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| PageText {
            page: u32::try_from(p["page"].as_u64().unwrap()).unwrap(),
            text: p["text"].as_str().unwrap().into(),
            origin: TextOrigin::Ocr,
        })
        .collect();
    Extracted {
        pages,
        fields: vec![],
    }
}

fn one_page(text: &str) -> Extracted {
    Extracted {
        pages: vec![PageText {
            page: 1,
            text: text.into(),
            origin: TextOrigin::TextLayer,
        }],
        fields: vec![],
    }
}

fn values<'a>(e: &'a Extracted, dets: &[Detection]) -> Vec<&'a str> {
    dets.iter()
        .filter(|d| d.field.is_none())
        .map(|d| {
            let p = e.pages.iter().find(|p| p.page == d.page).unwrap();
            &p.text[d.span.start..d.span.end]
        })
        .collect()
}

/// The detection whose text is exactly `v`.
fn found<'a>(e: &Extracted, dets: &'a [Detection], v: &str) -> &'a Detection {
    dets.iter()
        .filter(|d| d.field.is_none())
        .find(|d| {
            let p = e.pages.iter().find(|p| p.page == d.page).unwrap();
            &p.text[d.span.start..d.span.end] == v
        })
        .unwrap_or_else(|| panic!("{v:?} not inked; inked: {:?}", values(e, dets)))
}

fn run(e: &Extracted) -> (Vec<Detection>, Classification) {
    detect(e, &packs()).unwrap()
}

fn pin() -> TokenKind {
    TokenKind::Custom("pin".into())
}

/// No inked span touches `v` (wherever it occurs on the page).
fn untouched(e: &Extracted, dets: &[Detection], v: &str) {
    for p in &e.pages {
        for (at, _) in p.text.match_indices(v) {
            let end = at + v.len();
            assert!(
                !dets
                    .iter()
                    .any(|d| d.page == p.page && d.span.start < end && at < d.span.end),
                "{v:?} was inked"
            );
        }
    }
}

// --- Form 8879 ------------------------------------------------------------

#[test]
fn the_8879_inks_every_identifier_and_every_pin() {
    let e = text_8879();
    let (dets, _) = run(&e);
    for (v, kind) in [
        ("900-12-3456", TokenKind::Ssn),
        ("666-48-2210", TokenKind::Ssn),
        ("Jordan A Example", TokenKind::Person),
        ("Riley Sample", TokenKind::Person),
        ("13579", pin()),
        ("24680", pin()),
        ("00000097531", pin()),
    ] {
        assert_eq!(found(&e, &dets, v).kind, kind, "{v}");
    }
    // The firm is named once per signer; both are inked.
    let firm: Vec<_> = dets
        .iter()
        .filter(|d| &e.pages[0].text[d.span.start..d.span.end] == "Ledgerline Tax Services LLC")
        .collect();
    assert_eq!(firm.len(), 2, "inked: {:?}", values(&e, &dets));
    assert!(firm.iter().all(|d| d.kind == TokenKind::Org));
}

#[test]
fn the_8879_is_recognised_with_roles_and_nothing_to_ask() {
    let e = text_8879();
    let (dets, class) = run(&e);
    assert_eq!(class.kind.as_str(), "8879");
    assert_eq!(class.year, Some(2023));
    for (v, role) in [
        ("900-12-3456", "taxpayer"),
        ("666-48-2210", "spouse"),
        ("Jordan A Example", "taxpayer"),
        ("Riley Sample", "spouse"),
        ("00000097531", "preparer"),
        ("Ledgerline Tax Services LLC", "preparer"),
    ] {
        let d = found(&e, &dets, v);
        assert!(
            matches!(d.source, DetectionSource::Template { .. }),
            "{v}: {:?}",
            d.source
        );
        assert_eq!(d.role.as_deref().unwrap_or(""), role, "{v}");
    }
    let held: Vec<_> = dets.iter().filter(|d| d.needs_review).collect();
    assert!(
        held.is_empty(),
        "a clean 8879 is not held: {:?}",
        values(&e, &dets)
    );
}

#[test]
fn the_8879_keeps_its_amounts_and_form_numbers() {
    let e = text_8879();
    let (dets, _) = run(&e);
    for v in [
        "103,140",
        "9,242",
        "11,912",
        "2,670",
        "1545-0074",
        "32778X",
        "Form 8879 (2023)",
    ] {
        untouched(&e, &dets, v);
    }
}

#[test]
fn the_8879_scan_inks_every_identifier_and_every_pin() {
    let e = ocr_8879();
    let (dets, class) = run(&e);
    assert_eq!(class.kind.as_str(), "8879");
    for v in [
        "900123456",
        "666 48 2210",
        "Jordan A Example",
        "Riley Sample",
        "13579",
        "24680",
        "00000097531",
    ] {
        found(&e, &dets, v);
    }
}

/// The 8879 as a PDF whose glyphs are placed one by one: its text layer
/// reads back letter-spaced ("9 0 0 - 1 2 - 3 4 5 6").
fn glyph_8879() -> Extracted {
    use inker_core::ports::TextSource;
    let path = camino::Utf8PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../samples/synthetic/8879-example-2023-glyphs.pdf"
    ));
    inker_extract::PdfTextSource::new()
        .extract(&path)
        .unwrap()
        .expect("a text layer")
}

/// Every place `v` occurs with any single spaces between its characters.
fn spaced_occurrences(text: &str, v: &str) -> Vec<(usize, usize)> {
    let pattern: Vec<String> = v
        .chars()
        .filter(|c| !c.is_whitespace())
        .map(|c| regex::escape(&c.to_string()))
        .collect();
    regex::Regex::new(&pattern.join(" ?"))
        .unwrap()
        .find_iter(text)
        .map(|m| (m.start(), m.end()))
        .collect()
}

#[test]
fn the_8879_read_back_letter_spaced_is_recognised_and_inked() {
    // The reported document: a text layer placed glyph by glyph read back
    // letter-spaced, so no stage found anything and it was served with
    // nothing inked, filed as "doc" with no year.
    let e = glyph_8879();
    let text = &e.pages[0].text;
    assert!(
        text.contains("9 0 0 - 1 2 - 3 4 5 6"),
        "the sample reads back letter-spaced"
    );
    let (dets, class) = run(&e);
    assert_eq!((class.kind.as_str(), class.year), ("8879", Some(2023)));
    for v in [
        "900-12-3456",
        "666-48-2210",
        "Jordan A Example",
        "Riley Sample",
        "Ledgerline Tax Services LLC",
        "13579",
        "24680",
        "00000097531",
    ] {
        let at = spaced_occurrences(text, v);
        assert!(!at.is_empty(), "{v} not in the sample");
        for (s, e) in at {
            let mut covered: Vec<(usize, usize)> = dets
                .iter()
                .filter(|d| d.field.is_none() && d.span.start < e && s < d.span.end)
                .map(|d| (d.span.start, d.span.end))
                .collect();
            covered.sort_unstable();
            let mut pos = s;
            for (cs, ce) in covered {
                if cs <= pos {
                    pos = pos.max(ce);
                }
            }
            assert!(pos >= e, "{v}: {:?} is not fully inked", &text[s..e]);
        }
    }
}

#[test]
fn no_page_of_the_8879_is_left_with_nothing_inked() {
    for e in [text_8879(), ocr_8879(), glyph_8879()] {
        let (dets, _) = run(&e);
        for p in &e.pages {
            assert!(
                dets.iter().any(|d| d.page == p.page && d.field.is_none()),
                "page {} has nothing inked",
                p.page
            );
        }
    }
}

// --- PINs on pages no template matches --------------------------------------

#[test]
fn a_pin_after_a_pin_label_is_inked_without_a_template() {
    for (text, v, kind) in [
        ("Signature PIN 13579 on file.", "13579", pin()),
        ("Taxpayer's PIN: 13579", "13579", pin()),
        ("Spouse's PIN   24680", "24680", pin()),
        (
            "Taxpayer's PIN: check one box only\nmy PIN   13579",
            "13579",
            pin(),
        ),
        ("ERO's EFIN/PIN: 00000097531", "00000097531", pin()),
        ("Self-select PIN (5 digits): 24680", "24680", pin()),
        (
            "Identity Protection PIN 000417",
            "000417",
            TokenKind::Custom("ip_pin".into()),
        ),
        (
            "IP PIN: 000417",
            "000417",
            TokenKind::Custom("ip_pin".into()),
        ),
        (
            "Third party designee   Personal identification number (PIN)   40917",
            "40917",
            pin(),
        ),
        (
            "Personal identification number (PIN)\n  40917",
            "40917",
            pin(),
        ),
    ] {
        let e = one_page(text);
        let (dets, _) = run(&e);
        let d = found(&e, &dets, v);
        assert_eq!(d.kind, kind, "{text:?}");
        assert!(
            matches!(&d.source, DetectionSource::Rule { name } if name.contains("pin")),
            "{text:?}: {:?}",
            d.source
        );
        assert!(
            !d.needs_review,
            "{text:?}: a labelled PIN is not a question"
        );
    }
}

#[test]
fn numbers_that_are_not_pins_are_left_alone() {
    for text in [
        // No PIN label.
        "Invoice 13579 paid in full.",
        // An amount after the label.
        "PIN fee $13,579.00 waived",
        // A word that only contains the letters.
        "SPINDLE 13579",
        // The wrong number of digits for any PIN.
        "PIN 1357",
        "PIN 1357924",
        // Far from the label, several lines down.
        "PIN\n\n\n\nRoom 13579",
    ] {
        let e = one_page(text);
        let (dets, _) = run(&e);
        let pins: Vec<_> = dets
            .iter()
            .filter(|d| matches!(&d.source, DetectionSource::Rule { name } if name.contains("pin")))
            .collect();
        assert!(pins.is_empty(), "{text:?}: {:?}", values(&e, &dets));
    }
}

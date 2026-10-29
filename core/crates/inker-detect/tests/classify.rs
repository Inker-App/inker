#![allow(clippy::unwrap_used, clippy::expect_used)] // tests may panic

use std::path::PathBuf;

use inker_core::{DetectionSource, DocKind, Extracted, PageText, TextOrigin, TokenKind, Treatment};
use inker_detect::{classify, detection, template};
use inker_packs::PackSet;

fn doc(pages: &[(&str, TextOrigin)]) -> Extracted {
    Extracted {
        pages: pages
            .iter()
            .enumerate()
            .map(|(i, (t, o))| PageText {
                page: i as u32 + 1,
                text: (*t).into(),
                origin: *o,
            })
            .collect(),
        fields: vec![],
    }
}

fn kind(c: &classify::Classification) -> &str {
    c.kind.as_str()
}

#[test]
fn template_gives_kind_year_and_org() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../packs/tax");
    let set = PackSet::load(Some(&dir)).unwrap();
    let text = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/w2.txt"),
    )
    .unwrap();
    let e = doc(&[(&text, TextOrigin::TextLayer)]);
    let m = template::match_template(&e, &set).unwrap();
    let out = template::apply(&e, &m);
    let c = classify::classify(&e, Some(&m), &out.detections);
    assert_eq!(c.kind, DocKind("w2".into()));
    assert_eq!(c.year, Some(2025));
    assert_eq!(c.org.as_deref(), Some("Contoso Widgets LLC"));
    assert_eq!(c.tag.as_deref(), Some("Taxes"), "the template's tag");
}

#[test]
fn heuristic_kinds_carry_their_own_tags() {
    let e = doc(&[(BANK, TextOrigin::TextLayer)]);
    assert_eq!(
        classify::classify(&e, None, &[]).tag.as_deref(),
        Some("Banking")
    );
    let e = doc(&[(
        "Closing Disclosure\nfinal loan terms",
        TextOrigin::TextLayer,
    )]);
    assert_eq!(
        classify::classify(&e, None, &[]).tag.as_deref(),
        Some("Legal")
    );
    let e = doc(&[("Form 1099-SA OMB No. 1545-1517", TextOrigin::TextLayer)]);
    assert_eq!(
        classify::classify(&e, None, &[]).tag.as_deref(),
        Some("Taxes")
    );
    for t in ["RECEIPT\nCoffee 4.50\nTotal 4.50", "nothing much"] {
        let e = doc(&[(t, TextOrigin::TextLayer)]);
        assert_eq!(classify::classify(&e, None, &[]).tag, None, "{t}");
    }
    let e = doc(&[("blurry", TextOrigin::Ocr)]);
    assert_eq!(classify::classify(&e, None, &[]).tag, None);
}

const BANK: &str = "Northwind Bank\nStatement period: 01/01/2025 - 01/31/2025\nBeginning balance 1,200.00\nDeposits 400.00\nWithdrawals 150.00\nEnding balance 1,450.00\n";

#[test]
fn bank_statement() {
    let e = doc(&[(BANK, TextOrigin::TextLayer)]);
    let org = detection(
        1,
        0,
        14,
        TokenKind::Org,
        DetectionSource::Rule {
            name: "org_suffix".into(),
        },
        0.45,
        Treatment::Tokenize,
    );
    let c = classify::classify(&e, None, &[org]);
    assert_eq!(kind(&c), "bank-statement");
    assert_eq!(c.year, Some(2025));
    // The org goes into the codename: only a template field may supply it.
    assert_eq!(c.org, None);
}

#[test]
fn heuristics_beat_scan() {
    // An OCR'd bank statement is still a bank statement.
    let e = doc(&[(BANK, TextOrigin::Ocr)]);
    assert_eq!(kind(&classify::classify(&e, None, &[])), "bank-statement");
}

#[test]
fn closing_statements() {
    for t in [
        "Closing Disclosure\nThis form is a statement of final loan terms and closing costs. 2025",
        "A. Settlement Statement (HUD-1)\nB. Type of Loan",
        "Seller's Closing Statement\nProperty: 1428 Elm Street",
    ] {
        let e = doc(&[(t, TextOrigin::TextLayer)]);
        assert_eq!(
            kind(&classify::classify(&e, None, &[])),
            "closing-statement",
            "{t}"
        );
    }
}

#[test]
fn receipts() {
    for t in [
        "RECEIPT\nCoffee 4.50\nTotal 4.50",
        "Item 1 9.99\nSubtotal 9.99\nTax 0.80\nTotal 10.79",
    ] {
        let e = doc(&[(t, TextOrigin::TextLayer)]);
        assert_eq!(kind(&classify::classify(&e, None, &[])), "receipt", "{t}");
    }
}

#[test]
fn scans_and_unknowns() {
    let e = doc(&[
        ("blurry words", TextOrigin::Ocr),
        ("more blurry words", TextOrigin::Ocr),
    ]);
    assert_eq!(kind(&classify::classify(&e, None, &[])), "scan");
    let e = doc(&[
        ("blurry words", TextOrigin::Ocr),
        ("a typed page", TextOrigin::TextLayer),
    ]);
    assert_eq!(kind(&classify::classify(&e, None, &[])), "unknown");
    let e = doc(&[]);
    let c = classify::classify(&e, None, &[]);
    assert_eq!(kind(&c), "unknown");
    assert_eq!(c.year, None);
    assert_eq!(c.org, None);
}

#[test]
fn year_is_the_most_common_one() {
    let e = doc(&[(
        "Letter dated 2025. Re: 2025 taxes. Since 2019. Year 2025.",
        TextOrigin::TextLayer,
    )]);
    assert_eq!(classify::classify(&e, None, &[]).year, Some(2025));
    let e = doc(&[("Account 992025123 ref 20250", TextOrigin::TextLayer)]);
    assert_eq!(
        classify::classify(&e, None, &[]).year,
        None,
        "years inside numbers do not count"
    );
}

fn tsrc() -> DetectionSource {
    DetectionSource::Template {
        form: "demo".into(),
        field: "payer".into(),
    }
}

#[test]
fn org_comes_only_from_template_fields_with_an_org_role() {
    let text = "Alpha Bank and Beta Trust";
    let e = doc(&[(text, TextOrigin::TextLayer)]);
    let rule = || DetectionSource::Rule { name: "x".into() };
    let fail = || DetectionSource::FailClosed { reason: "x".into() };
    let a = detection(1, 0, 10, TokenKind::Org, rule(), 0.5, Treatment::Tokenize);
    let mut b = detection(1, 15, 25, TokenKind::Org, fail(), 0.5, Treatment::Tokenize);
    b.role = Some("payer".into());
    assert_eq!(
        classify::classify(&e, None, &[a.clone(), b.clone()]).org,
        None
    );
    // A template org with no org role does not name the document either.
    let mut c = detection(1, 0, 10, TokenKind::Org, tsrc(), 0.9, Treatment::Tokenize);
    c.role = Some("recipient".into());
    assert_eq!(
        classify::classify(&e, None, &[a.clone(), c.clone()]).org,
        None
    );
    c.role = Some("payer".into());
    let mut d = detection(1, 15, 25, TokenKind::Org, tsrc(), 0.9, Treatment::Tokenize);
    d.role = Some("lender".into());
    assert_eq!(
        classify::classify(&e, None, &[a, b, d, c]).org.as_deref(),
        Some("Alpha Bank")
    );
}

#[test]
fn names_pass_org_is_not_the_org_without_a_template() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../packs/tax");
    let set = PackSet::load(Some(&dir)).unwrap();
    let e = doc(&[(BANK, TextOrigin::TextLayer)]);
    let (dets, c) = inker_detect::detect(&e, &set).unwrap();
    assert!(
        dets.iter().any(|d| d.kind == TokenKind::Org),
        "names pass found the bank"
    );
    assert_eq!(c.org, None);
}

#[test]
fn org_can_come_from_a_form_field() {
    let mut e = doc(&[("nothing here", TextOrigin::TextLayer)]);
    e.fields.push(inker_core::FormField {
        name: "f1_01".into(),
        value: " Contoso Widgets LLC ".into(),
        page: Some(1),
    });
    let mut d = detection(1, 1, 20, TokenKind::Org, tsrc(), 0.9, Treatment::Tokenize);
    d.field = Some("f1_01".into());
    d.role = Some("employer".into());
    assert_eq!(
        classify::classify(&e, None, &[d]).org.as_deref(),
        Some("Contoso Widgets LLC")
    );
}

#[test]
fn debug_does_not_print_the_org() {
    let c = classify::Classification {
        kind: DocKind("w2".into()),
        year: Some(2025),
        org: Some("Contoso Widgets LLC".into()),
        tag: Some("Taxes".into()),
    };
    let s = format!("{c:?}");
    assert!(!s.contains("Contoso"), "{s}");
    assert!(
        s.contains("w2") && s.contains("2025") && s.contains("org: Some(<redacted>)"),
        "{s}"
    );
    let none = classify::Classification { org: None, ..c };
    assert!(format!("{none:?}").contains("org: None"));
}

#[test]
fn untemplated_tax_forms() {
    for t in [
        "Form 1099-R  Distributions From Pensions, Annuities 2025\nOMB No. 1545-0119\n",
        "Form 1099-SA Distributions From an HSA 2025 OMB No. 1545-1517",
        "Form W-2G Certain Gambling Winnings OMB No. 1545-0238",
        "Form 1098-T Tuition Statement\nOMB No. 1545-1574",
    ] {
        let e = doc(&[(t, TextOrigin::TextLayer)]);
        assert_eq!(
            kind(&classify::classify(&e, None, &[])),
            classify::TAX_FORM,
            "{t}"
        );
    }
    // A mention without the OMB line is not a tax form.
    let e = doc(&[("Attach Form 1099-R to your return.", TextOrigin::TextLayer)]);
    assert_eq!(kind(&classify::classify(&e, None, &[])), "unknown");
}

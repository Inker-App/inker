//! End-to-end: `detect` over synthetic fixtures, plus merge and model hooks.
//!
//! The routing number 011200446 in bank-statement.txt and pii-check: allow (synthetic, constructed for the checksum test)
//! closing-statement.txt is synthetic: constructed to pass the ABA checksum.

#![allow(clippy::unwrap_used, clippy::expect_used)] // tests may panic

use std::path::PathBuf;

use inker_core::{
    Detection, DetectionSource, Extracted, PageText, Result, TextOrigin, TokenKind, Treatment,
};
use inker_detect::{Classification, Detector, Model, NoModel, detect, detection, merge};
use inker_packs::PackSet;

fn packs() -> PackSet {
    PackSet::load(Some(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../packs/tax"),
    ))
    .unwrap()
}

fn fixture(name: &str) -> String {
    std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name),
    )
    .unwrap()
}

fn one_page(text: &str, origin: TextOrigin) -> Extracted {
    Extracted {
        pages: vec![PageText {
            page: 1,
            text: text.into(),
            origin,
        }],
        fields: vec![],
    }
}

fn run(name: &str, origin: TextOrigin) -> (String, Vec<Detection>, Classification) {
    let text = fixture(name);
    let (d, c) = detect(&one_page(&text, origin), &packs()).unwrap();
    (text, d, c)
}

fn value<'a>(text: &'a str, d: &Detection) -> &'a str {
    &text[d.span.start..d.span.end]
}

fn find<'a>(text: &str, dets: &'a [Detection], v: &str) -> &'a Detection {
    dets.iter()
        .find(|d| value(text, d) == v)
        .unwrap_or_else(|| {
            panic!(
                "no detection for {v:?}; have {:?}",
                dets.iter().map(|d| value(text, d)).collect::<Vec<_>>()
            )
        })
}

fn untouched(text: &str, dets: &[Detection], amounts: &[&str]) {
    for a in amounts {
        let at = text.find(a).unwrap_or_else(|| panic!("{a} not in fixture"));
        let end = at + a.len();
        assert!(
            !dets.iter().any(|d| d.span.start < end && at < d.span.end),
            "{a} was touched"
        );
    }
}

fn no_overlaps(dets: &[Detection]) {
    for (i, a) in dets.iter().enumerate() {
        for b in &dets[i + 1..] {
            if a.page == b.page && a.field == b.field {
                assert!(
                    a.span.end <= b.span.start || b.span.end <= a.span.start,
                    "overlap {a:?} {b:?}"
                );
            }
        }
    }
}

#[test]
fn w2_end_to_end() {
    let (text, dets, c) = run("w2.txt", TextOrigin::TextLayer);
    assert_eq!(c.kind.as_str(), "w2");
    assert_eq!(c.year, Some(2025));
    assert_eq!(c.org.as_deref(), Some("Contoso Widgets LLC"));
    no_overlaps(&dets);
    let ssn = find(&text, &dets, "900-14-2231");
    assert_eq!(
        (ssn.kind.clone(), ssn.role.as_deref()),
        (TokenKind::Ssn, Some("employee"))
    );
    assert!(
        matches!(ssn.source, DetectionSource::Template { .. }),
        "template beats the SSN rule"
    );
    assert_eq!(
        find(&text, &dets, "07-4400123").role.as_deref(),
        Some("employer")
    );
    assert_eq!(find(&text, &dets, "Jane Q Doe").kind, TokenKind::Person);
    let home = find(&text, &dets, "1428 Elm Street Apt 4B\nBrooklyn, NY 11201");
    assert_eq!(
        (home.treatment, home.generalized.as_deref()),
        (Treatment::Generalize, Some("Brooklyn, NY"))
    );
    untouched(
        &text,
        &dets,
        &[
            "52340.18",
            "6120.44",
            "54000.00",
            "3348.00",
            "783.00",
            "1659.82",
            "2590.83",
            "1545-0029",
        ],
    );
    assert!(
        dets.iter().all(|d| !d.needs_review),
        "a clean W-2 is not held: {:?}",
        dets.iter()
            .filter(|d| d.needs_review)
            .map(|d| value(&text, d))
            .collect::<Vec<_>>()
    );
}

#[test]
fn f1099_int_end_to_end() {
    let (text, dets, c) = run("1099-int.txt", TextOrigin::TextLayer);
    assert_eq!((c.kind.as_str(), c.year), ("1099-int", Some(2025)));
    assert_eq!(c.org.as_deref(), Some("Northwind Credit Union"));
    no_overlaps(&dets);
    assert_eq!(
        find(&text, &dets, "912-34-5678").role.as_deref(),
        Some("recipient")
    );
    assert_eq!(
        find(&text, &dets, "17-2233445").role.as_deref(),
        Some("payer")
    );
    assert_eq!(find(&text, &dets, "(608) 555-0199").kind, TokenKind::Phone);
    assert_eq!(find(&text, &dets, "5501-2299-87").kind, TokenKind::Account);
    untouched(&text, &dets, &["1,204.55", "88.10", "31.20", "1545-0112"]);
}

#[test]
fn f1099_nec_end_to_end() {
    let (text, dets, c) = run("1099-nec.txt", TextOrigin::TextLayer);
    assert_eq!((c.kind.as_str(), c.year), ("1099-nec", Some(2025)));
    assert_eq!(c.org.as_deref(), Some("Fabrikam Consulting Inc"));
    no_overlaps(&dets);
    assert_eq!(
        find(&text, &dets, "Pat R Example").role.as_deref(),
        Some("recipient")
    );
    let city = find(&text, &dets, "Boise, ID 83702");
    assert_eq!(city.generalized.as_deref(), Some("Boise, ID"));
    untouched(&text, &dets, &["18,250.00", "612.00"]);
}

#[test]
fn bank_statement_end_to_end() {
    let (text, dets, c) = run("bank-statement.txt", TextOrigin::TextLayer);
    assert_eq!((c.kind.as_str(), c.year), ("bank-statement", Some(2025)));
    assert_eq!(
        c.org, None,
        "no template: the org stays out of the codename"
    );
    no_overlaps(&dets);
    let kinds = [
        ("Northwind Bank", TokenKind::Org),
        ("7700012345678", TokenKind::Account),
        ("011200446", TokenKind::Routing), // pii-check: allow (synthetic, constructed for the checksum test)
        ("Jane Q Doe", TokenKind::Person),
        ("CONTOSO WIDGETS LLC", TokenKind::Org),
        ("(614) 555-0142", TokenKind::Phone),
        ("help@northwind.example", TokenKind::Email),
    ];
    for (v, k) in kinds {
        let d = find(&text, &dets, v);
        assert_eq!(d.kind, k, "{v}");
        assert_eq!(d.treatment, Treatment::Tokenize, "{v}");
        assert!(!d.needs_review, "{v}");
    }
    let home = find(&text, &dets, "1428 Elm Street Apt 4B\nBrooklyn, NY 11201");
    assert_eq!(home.generalized.as_deref(), Some("Brooklyn, NY"));
    let bank = find(&text, &dets, "PO Box 4471\nColumbus, OH 43215");
    assert_eq!(bank.generalized.as_deref(), Some("Columbus, OH"));
    // A transaction reference is inked, but does not ask a question.
    let r = find(&text, &dets, "8812004471923");
    assert!(!r.needs_review);
    assert_eq!(r.treatment, Treatment::Tokenize);
    assert!(matches!(r.source, DetectionSource::FailClosed { .. }));
    untouched(
        &text,
        &dets,
        &[
            "1,200.00",
            "400.00",
            "1,450.00",
            "2,100.00",
            "-150.00",
            "-500.00",
            "01/01/2025",
        ],
    );
}

#[test]
fn closing_statement_rough_scan() {
    let (text, dets, c) = run("closing-statement.txt", TextOrigin::Ocr);
    assert_eq!((c.kind.as_str(), c.year), ("closing-statement", Some(2025)));
    assert_eq!(
        c.org, None,
        "no template: the org stays out of the codename"
    );
    no_overlaps(&dets);
    let b = find(&text, &dets, "Maria Example-Lopez");
    assert_eq!(
        (b.kind.clone(), b.role.as_deref()),
        (TokenKind::Person, Some("borrower"))
    );
    assert_eq!(find(&text, &dets, "Sam T Sample").kind, TokenKind::Person);
    assert_eq!(
        find(&text, &dets, "Adatum Mortgage Co.").role.as_deref(),
        Some("lender")
    );
    assert_eq!(
        find(&text, &dets, "4471-0099-3311").kind,
        TokenKind::Account
    );
    assert_eq!(find(&text, &dets, "77001234556").kind, TokenKind::Account);
    let wire = find(&text, &dets, "011200446"); // pii-check: allow (synthetic, constructed for the checksum test)
    assert_eq!(
        (wire.kind.clone(), wire.needs_review),
        (TokenKind::Routing, false)
    );
    let prop = find(&text, &dets, "61 Willow Ct, Tucson, AZ 85701");
    assert_eq!(prop.generalized.as_deref(), Some("Tucson, AZ"));
    // The garbled wire line: held, as a routing number, without the value in the prompt.
    let rough = find(&text, &dets, "0l12OO446");
    assert!(rough.needs_review);
    assert_eq!(rough.kind, TokenKind::Routing);
    assert_eq!(
        rough.review_prompt.as_deref(),
        Some("Looks like a routing number, but the scan is rough. Ink it?")
    );
    untouched(&text, &dets, &["412,000.00", "84,210.55", "03/14/2025"]);
}

#[test]
fn empty_input_is_unknown() {
    let (d, c) = detect(&Extracted::default(), &packs()).unwrap();
    assert!(d.is_empty());
    assert_eq!(c.kind.as_str(), "unknown");
}

#[test]
fn acroform_fields_are_scanned_not_passed_through() {
    // A W-2 page whose template maps no AcroForm names: an unmapped field
    // that is not an amount is inked; an amount stays clear.
    let text = fixture("w2.txt");
    let mut e = one_page(&text, TextOrigin::TextLayer);
    for (name, value) in [("unrelated", "12.00"), ("f1_11", "666-12-3456")] {
        e.fields.push(inker_core::FormField {
            name: name.into(),
            value: value.into(),
            page: Some(1),
        });
    }
    let (d, _) = detect(&e, &packs()).unwrap();
    assert!(!d.iter().any(|d| d.field.as_deref() == Some("unrelated")));
    let ssn = d
        .iter()
        .find(|d| d.field.as_deref() == Some("f1_11"))
        .expect("unmapped field is scanned");
    assert_eq!((ssn.span.start, ssn.span.end), (0, 11));
}

#[test]
fn keep_box_does_not_shelter_a_reference_number() {
    // Probe from the review: a keep box's line carries more than the amount.
    let text = fixture("w2.txt").replace(
        "52340.18\n2 Federal",
        "52000.00 ref 4471002233119\n2 Federal",
    );
    let (dets, _) = detect(&one_page(&text, TextOrigin::TextLayer), &packs()).unwrap();
    let r = find(&text, &dets, "4471002233119");
    assert!(r.needs_review, "the reference number holds the document");
    untouched(&text, &dets, &["52000.00"]);
}

fn src(kind: &str) -> DetectionSource {
    match kind {
        "t" => DetectionSource::Template {
            form: "w2".into(),
            field: "a".into(),
        },
        "r" => DetectionSource::Rule { name: "r".into() },
        "m" => DetectionSource::Model { name: "m".into() },
        _ => DetectionSource::FailClosed { reason: "f".into() },
    }
}

fn d(page: u32, s: usize, e: usize, source: &str, conf: f32) -> Detection {
    detection(
        page,
        s,
        e,
        TokenKind::Account,
        src(source),
        conf,
        Treatment::Tokenize,
    )
}

#[test]
fn merge_prefers_precision_then_length_then_confidence() {
    let out = merge(vec![
        d(1, 5, 10, "t", 0.95), // fail-closed inside the template is dropped
        d(1, 6, 9, "f", 0.4),
        d(1, 30, 40, "r", 0.5), // model inside the rule is dropped
        d(1, 32, 38, "m", 0.99),
        d(1, 50, 60, "r", 0.5), // same source: longer wins
        d(1, 50, 65, "r", 0.5),
        d(1, 70, 80, "r", 0.5), // same span: higher confidence wins
        d(1, 70, 80, "r", 0.9),
        d(2, 5, 10, "f", 0.4), // other page: no conflict
    ]);
    let got: Vec<(u32, usize, usize, f32)> = out
        .iter()
        .map(|d| (d.page, d.span.start, d.span.end, d.confidence))
        .collect();
    assert_eq!(
        got,
        vec![
            (1, 5, 10, 0.95),
            (1, 30, 40, 0.5),
            (1, 50, 65, 0.5),
            (1, 70, 80, 0.9),
            (2, 5, 10, 0.4)
        ]
    );
}

#[test]
fn merge_unions_partial_overlaps_when_tokenizing() {
    // Template 5..10 and a fail-closed 0..20 around it: nothing is dropped.
    let mut t = d(1, 5, 10, "t", 0.95);
    t.kind = TokenKind::Ssn;
    t.role = Some("employee".into());
    let out = merge(vec![d(1, 0, 20, "f", 0.4), t]);
    assert_eq!(out.len(), 1, "{out:?}");
    let u = &out[0];
    assert_eq!((u.span.start, u.span.end), (0, 20));
    assert_eq!(u.treatment, Treatment::Tokenize);
    assert_eq!(u.kind, TokenKind::Ssn, "the more precise source's kind");
    assert_eq!(u.role.as_deref(), Some("employee"));
    assert!(matches!(u.source, DetectionSource::Template { .. }));

    // Rule 30..40 and model 35..45 overlap partly: the union is inked.
    let out = merge(vec![d(1, 35, 45, "m", 0.99), d(1, 30, 40, "r", 0.5)]);
    assert_eq!(out.len(), 1);
    assert_eq!((out[0].span.start, out[0].span.end), (30, 45));
    assert!(matches!(out[0].source, DetectionSource::Rule { .. }));

    // A generalized address partly overlapped by a tokenized hit: inked whole.
    let mut a = d(1, 0, 10, "t", 0.95);
    a.treatment = Treatment::Generalize;
    a.generalized = Some("Brooklyn, NY".into());
    let out = merge(vec![a, d(1, 8, 14, "r", 0.9)]);
    assert_eq!(out.len(), 1);
    assert_eq!(
        (out[0].span.start, out[0].span.end, out[0].treatment),
        (0, 14, Treatment::Tokenize)
    );
    assert_eq!(out[0].generalized, None);

    // A union that reaches a third detection absorbs it too.
    let out = merge(vec![
        d(1, 0, 10, "r", 0.9),
        d(1, 8, 22, "f", 0.4),
        d(1, 20, 30, "r", 0.8),
    ]);
    assert_eq!(out.len(), 1, "{out:?}");
    assert_eq!((out[0].span.start, out[0].span.end), (0, 30));
}

#[test]
fn merge_keeps_what_a_template_keep_does_not_cover() {
    let mut k = d(1, 10, 20, "t", 0.95);
    k.treatment = Treatment::Keep;
    let out = merge(vec![k.clone(), d(1, 5, 25, "r", 0.9)]);
    let got: Vec<(usize, usize, Treatment)> = out
        .iter()
        .map(|d| (d.span.start, d.span.end, d.treatment))
        .collect();
    assert_eq!(
        got,
        vec![
            (5, 10, Treatment::Tokenize),
            (10, 20, Treatment::Keep),
            (20, 25, Treatment::Tokenize)
        ]
    );
    // A tokenized hit wholly inside the keep stays dropped.
    let out = merge(vec![k, d(1, 12, 18, "r", 0.9)]);
    assert_eq!(out.len(), 1);
}

#[test]
fn merge_keeps_form_fields_apart_from_page_text() {
    let mut a = d(1, 0, 5, "t", 0.9);
    a.field = Some("f1_01".into());
    let b = d(1, 0, 5, "r", 0.9);
    let out = merge(vec![a, b]);
    assert_eq!(out.len(), 2);
    assert!(out[0].field.is_none() && out[1].field.is_some());
}

#[derive(Debug)]
struct FakeModel;

impl Model for FakeModel {
    fn detect(&self, pages: &[PageText]) -> Result<Vec<Detection>> {
        // A name the rules do not find, and a span the rules already own.
        let t = &pages[0].text;
        let at = t.find("Zelle").unwrap_or(0);
        let acct = t.find("7700012345678").unwrap_or(0);
        Ok(vec![
            detection(
                1,
                at,
                at + 5,
                TokenKind::Org,
                src("m"),
                0.8,
                Treatment::Tokenize,
            ),
            detection(
                1,
                acct,
                acct + 13,
                TokenKind::Custom("x".into()),
                src("m"),
                0.99,
                Treatment::Tokenize,
            ),
        ])
    }
}

#[derive(Debug)]
struct BrokenModel;

impl Model for BrokenModel {
    fn detect(&self, _: &[PageText]) -> Result<Vec<Detection>> {
        Err(inker_core::Error::Detect("model failed".into()))
    }
}

#[test]
fn model_detections_join_the_merge() {
    let text = fixture("bank-statement.txt");
    let set = packs();
    let e = one_page(&text, TextOrigin::TextLayer);
    let (dets, _) = Detector::new(&set)
        .with_model(Box::new(FakeModel))
        .detect(&e)
        .unwrap();
    let z = find(&text, &dets, "Zelle");
    assert!(matches!(z.source, DetectionSource::Model { .. }));
    assert_eq!(
        find(&text, &dets, "7700012345678").kind,
        TokenKind::Account,
        "rule beats model"
    );
    assert!(
        Detector::new(&set)
            .with_model(Box::new(BrokenModel))
            .detect(&e)
            .is_err()
    );
}

#[test]
fn no_model_finds_nothing() {
    let pages = vec![PageText {
        page: 1,
        text: "Jane Doe".into(),
        origin: TextOrigin::TextLayer,
    }];
    assert!(NoModel.detect(&pages).unwrap().is_empty());
}

#[test]
fn a_busy_statement_asks_one_question() {
    let (text, dets, c) = run("bank-statement-20.txt", TextOrigin::TextLayer);
    assert_eq!(c.kind.as_str(), "bank-statement");
    no_overlaps(&dets);
    let held: Vec<&str> = dets
        .iter()
        .filter(|d| d.needs_review)
        .map(|d| value(&text, d))
        .collect();
    assert!(held.len() <= 2, "{} questions: {held:?}", held.len());
    // Every reference is still inked.
    for r in [
        "9111222333",
        "990000441234567",
        "112-3456789-1234567",
        "8812004471923",
        "0123456789",
        "000048213337",
        "1037621994857263",
        "990123487",
        "990547123456789",
        "1004958372615",
        "810237465912",
    ] {
        let d = find(&text, &dets, r);
        assert_eq!(d.treatment, Treatment::Tokenize, "{r}");
    }
    // Bare ten-digit numbers shaped like phone numbers are phones.
    for p in ["6145550142", "6145550188", "614-555-0177"] {
        assert_eq!(find(&text, &dets, p).kind, TokenKind::Phone, "{p}");
    }
    untouched(&text, &dets, &["2,100.00", "-1,200.00", "812.00", "1042"]);
}

#[test]
fn public_agency_contacts_are_never_inked() {
    // The pack lists the agency lines the IRS prints; anyone else's number
    // or address is inked as before.
    let text = "Questions? Call the IRS at 1-800-829-1040 (TTY 800.829.4059) or\n\
                Harbor Street Credit Union at (503) 555-0142.\n\
                Forward phishing to phishing@irs.gov; reach us at help@harbor.example.\n";
    let (dets, _) = detect(&one_page(text, TextOrigin::TextLayer), &packs()).unwrap();
    untouched(
        text,
        &dets,
        &["1-800-829-1040", "800.829.4059", "phishing@irs.gov"],
    );
    assert_eq!(find(text, &dets, "(503) 555-0142").kind, TokenKind::Phone);
    assert_eq!(
        find(text, &dets, "help@harbor.example").kind,
        TokenKind::Email
    );
}

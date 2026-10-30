#![allow(clippy::unwrap_used, clippy::expect_used)] // tests may panic

use inker_core::{DetectionSource, PageText, TextOrigin, TokenKind, Treatment};
use inker_detect::failclosed;

/// (kind, value, prompt) for each fail-closed detection, checking the
/// invariants every one of them must hold.
fn found_with(
    text: &str,
    origin: TextOrigin,
    covered: &[(usize, usize)],
) -> Vec<(TokenKind, String, String)> {
    let page = PageText {
        page: 1,
        text: text.into(),
        origin,
    };
    failclosed::find(&page, covered)
        .into_iter()
        .map(|d| {
            let value = text[d.span.start..d.span.end].to_string();
            let prompt = d.review_prompt.clone().expect("a prompt");
            assert!(d.needs_review);
            assert!(matches!(d.source, DetectionSource::FailClosed { .. }));
            assert_eq!(d.treatment, Treatment::Tokenize);
            assert!(d.confidence < 0.5);
            assert!(!prompt.contains(&value), "prompt leaks the value");
            assert!(
                !prompt.chars().any(|c| c.is_ascii_digit()),
                "prompt has digits: {prompt}"
            );
            assert!(prompt.ends_with("Ink it?"), "{prompt}");
            (d.kind, value, prompt)
        })
        .collect()
}

fn found(text: &str) -> Vec<(TokenKind, String, String)> {
    found_with(text, TextOrigin::TextLayer, &[])
}

fn ocr(text: &str) -> Vec<(TokenKind, String, String)> {
    found_with(text, TextOrigin::Ocr, &[])
}

fn number() -> TokenKind {
    TokenKind::Custom("number".into())
}

#[test]
fn long_numbers_without_context() {
    let f = found("Note 4471002233119 attached");
    assert_eq!(
        f,
        vec![(
            number(),
            "4471002233119".into(),
            "A long number I can't place. Ink it?".into()
        )]
    );
    let f = found("note 4471-0022-3311.");
    assert_eq!(f[0].1, "4471-0022-3311");
    assert_eq!(
        found("Note 44710022 attached"),
        vec![],
        "eight digits is below the text threshold"
    );
}

#[test]
fn context_picks_the_kind_and_prompt() {
    let cases = [
        (
            "Account no 4471002233119",
            TokenKind::Account,
            "Looks like an account number, but I'm not sure. Ink it?",
        ),
        (
            "routing 990000123",
            TokenKind::Routing,
            "Looks like a routing number, but I'm not sure. Ink it?",
        ),
        (
            "SSN 0001122334455",
            TokenKind::Ssn,
            "Looks like a Social Security number, but I'm not sure. Ink it?",
        ),
        (
            "EIN 0011223344",
            TokenKind::Ein,
            "Looks like an employer ID number, but I'm not sure. Ink it?",
        ),
    ];
    for (t, k, prompt) in cases {
        let f = found(t);
        assert_eq!(f.len(), 1, "{t}");
        assert_eq!((&f[0].0, f[0].2.as_str()), (&k, prompt), "{t}");
    }
}

#[test]
fn ocr_pages_lower_the_threshold_and_read_misread_digits() {
    // Eight digits pass on OCR.
    let f = ocr("acct 77001234");
    assert_eq!(f[0].0, TokenKind::Account);
    assert_eq!(
        f[0].2,
        "Looks like an account number, but the scan is rough. Ink it?"
    );
    // l and O read as 1 and 0 inside a digit run.
    let f = ocr("…wire to 0l12OO446 ref 4471…");
    assert_eq!(
        f,
        vec![(
            TokenKind::Routing,
            "0l12OO446".into(),
            "Looks like a routing number, but the scan is rough. Ink it?".into()
        )]
    );
    let f = ocr("Note 4471002233119");
    assert_eq!(f[0].2, "A long number, and the scan is rough. Ink it?");
}

#[test]
fn amounts_dates_and_years_are_left_alone() {
    for t in [
        "Total 1,234,567,890.00",
        "Total 123456789.00",
        "Paid $123456789",
        "Brooklyn, NY 11201 2025",
        "Period 2025-01-01 to 2025-01-31",
        "Phone 212 555 0142",
        "Box 12 D 1659.82 2025",
    ] {
        assert_eq!(found(t), vec![], "{t}");
    }
}

#[test]
fn covered_spans_are_skipped() {
    let t = "Note 4471002233119 attached";
    assert_eq!(found_with(t, TextOrigin::TextLayer, &[(5, 18)]), vec![]);
    assert_eq!(
        found_with(t, TextOrigin::TextLayer, &[(9, 11)]),
        vec![],
        "any overlap counts"
    );
}

#[test]
fn names_near_sensitive_labels() {
    let f = found("Name   Pat Example   Page 2");
    assert_eq!(
        f,
        vec![(
            TokenKind::Person,
            "Pat Example".into(),
            "Looks like a name, but I'm not sure. Ink it?".into()
        )]
    );
    let f = found("Account name: Contoso Widgets LLC");
    assert_eq!(f[0].0, TokenKind::Org);
    let f = ocr("SSN of Chris Mock");
    assert_eq!(f[0].2, "Looks like a name, but the scan is rough. Ink it?");
    for t in [
        "Name: Form W-2",
        "Name: see attached",
        &format!("Name:{}Pat Example", " ".repeat(45)),
    ] {
        assert_eq!(found(t), vec![], "{t}");
    }
}

#[test]
fn digit_groups_join_across_dots_dashes_and_breaks() {
    for t in [
        "note 4471.0022.3311",
        "note 4471\u{2013}0022\u{2013}3311",
        "note 4471-0022-\n3311",
        "note \u{ff14}\u{ff14}\u{ff17}\u{ff11}\u{ff10}\u{ff10}\u{ff12}\u{ff12}\u{ff13}",
    ] {
        let f = found(t);
        assert_eq!(f.len(), 1, "{t:?}: {f:?}");
        assert_eq!(f[0].1, &t[5..], "{t:?}");
    }
    // An amount with a decimal point is still an amount.
    assert_eq!(found("Total 1234567.89"), vec![]);
}

#[test]
fn statement_references_are_inked_without_a_question() {
    let text = "PPD ID: 9111222333 TRACE 990000441234567 ORDER 112-3456789-1234567 \
                SEQ 990123487 x\nCASHOUT 1037621994857263 PPD\nIN 20250310000123 IMAD\n\
                REF NUMBER :810237465912\nconf 4471002233119\n\
                PAYMENTREC 000048213337\nTAX 990547123456789\nSSN ref 0001122334455\n";
    let pages = vec![PageText {
        page: 1,
        text: text.into(),
        origin: TextOrigin::TextLayer,
    }];
    let mut got = failclosed::find(&pages[0], &[]);
    assert!(got.iter().all(|d| d.needs_review), "held until settled");
    failclosed::settle_statement(&pages, &mut got);
    let v = |d: &inker_core::Detection| text[d.span.start..d.span.end].to_string();
    let refs: Vec<String> = got
        .iter()
        .filter(|d| matches!(&d.source, DetectionSource::FailClosed { reason } if reason == "transaction_ref"))
        .map(v)
        .collect();
    assert_eq!(refs.len(), 8, "{refs:?}");
    // The two unexplained numbers share one question; the SSN keeps its own.
    let asked: Vec<(String, Option<String>)> = got
        .iter()
        .filter(|d| d.needs_review)
        .map(|d| (v(d), d.review_prompt.clone()))
        .collect();
    assert_eq!(
        asked,
        vec![
            (
                "000048213337".into(),
                Some(failclosed::STATEMENT_PROMPT.into())
            ),
            (
                "0001122334455".into(),
                Some("Looks like a Social Security number, but I'm not sure. Ink it?".into())
            ),
        ]
    );
    assert!(got.iter().all(|d| d.treatment == Treatment::Tokenize));
}

// ---------------------------------------------------------------------------
// A real 1040 without a template: masked values and field labels are not
// names (Ben's first run asked about "Spouse's", "Preparer's" and X runs).
// ---------------------------------------------------------------------------

#[test]
fn masked_placeholders_are_not_names() {
    // Preparer and client copies print SSNs as X's.
    assert!(found("Your social security number X X X - X X - X X X X\n").is_empty());
    assert!(found("Spouse's social security number XXX-XX-XXXX\n").is_empty());
    assert!(found("Account number XXXXXXXX\n").is_empty());
}

#[test]
fn form_labels_after_a_label_are_not_names() {
    let text = "Your first name and middle initial Last name Your social security number\n\
                If joint return, spouse's first name and middle initial Last name Spouse's social security number\n\
                Home address (number and street). If you have a P.O. box, see instructions. Apt no.\n\
                Paid Preparer Use Only Preparer's name Preparer's signature Date PTIN\n\
                Firm's name Phone no. Firm's address Firm's EIN\n\
                Third Party Designee Designee's name Phone no. Personal identification number (PIN)\n";
    let f = found(text);
    assert!(f.is_empty(), "{f:?}");
}

#[test]
fn a_typed_name_after_a_label_is_still_held() {
    let f = found("Designee's name Riley Sample\n");
    assert!(
        f.iter()
            .any(|(k, v, _)| *k == TokenKind::Person && v == "Riley Sample"),
        "{f:?}"
    );
    // An organisation with a label word in it is still an organisation.
    let f = found("Account holder name Northwind Federal Credit Union\n");
    assert!(
        f.iter()
            .any(|(k, v, _)| *k == TokenKind::Org && v == "Northwind Federal Credit Union"),
        "{f:?}"
    );
}

#[test]
fn a_sentence_break_is_not_inside_a_name() {
    // Instructions prose: a label word, then the next sentence.
    for t in [
        "How to get a TIN. If you do not have a TIN, apply for one.",
        "your TIN is your IRS ITIN. Enter it in the entry space",
        "enter the name shown on your tax return. Individual. Generally, enter",
    ] {
        assert_eq!(found(t), vec![], "{t}");
    }
    // An initial with a dot is part of the name.
    let f = found("Name: Jordan A. Example");
    assert_eq!(f.len(), 1, "{f:?}");
    assert_eq!(f[0].1, "Jordan A. Example");
}

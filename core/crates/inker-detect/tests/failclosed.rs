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
    let f = found("Ref 4471002233119 attached");
    assert_eq!(
        f,
        vec![(
            number(),
            "4471002233119".into(),
            "A long number I can't place. Ink it?".into()
        )]
    );
    let f = found("ref 4471-0022-3311.");
    assert_eq!(f[0].1, "4471-0022-3311");
    assert_eq!(
        found("Ref 44710022 attached"),
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
    let f = ocr("Ref 4471002233119");
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
    let t = "Ref 4471002233119 attached";
    assert_eq!(found_with(t, TextOrigin::TextLayer, &[(4, 17)]), vec![]);
    assert_eq!(
        found_with(t, TextOrigin::TextLayer, &[(8, 10)]),
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

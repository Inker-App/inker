//! The unplaced-value pass on template pages: what no other stage placed
//! and that is shaped like a name or a number is held, never passed through.

#![allow(clippy::unwrap_used, clippy::expect_used)] // tests may panic

use inker_core::{DetectionSource, PageText, TextOrigin, TokenKind, Treatment};
use inker_detect::unplaced;
use inker_packs::{Pack, PackSet};

const MANIFEST: &str = r#"
name = "test"
version = "0.0.1"
forms = ["demo"]
"#;

const DEMO: &str = r#"
id = "demo"
display = "Demo"
kind = "demo"
anchors = ["Demo Statement"]

[[fields]]
id = "name"
label = "Recipient's name"
kind = "person"
treatment = "tokenize"

[[fields]]
id = "1"
label = '1\s+Gross Amount'
kind = "keep"
treatment = "keep"
"#;

fn set() -> PackSet {
    PackSet {
        packs: vec![
            Pack::from_sources(MANIFEST, &[("demo", DEMO)])
                .unwrap()
                .with_boilerplate(
                    "Copy B\nFor Recipient\nInternal Revenue Service\nBeginning  Ending\n",
                ),
        ],
    }
}

/// (kind, value, needs_review) for each detection, checking the invariants.
fn found_with(text: &str, covered: &[(usize, usize)]) -> Vec<(TokenKind, String, bool)> {
    let set = set();
    let page = PageText {
        page: 1,
        text: text.into(),
        origin: TextOrigin::TextLayer,
    };
    let t = set.forms().next().unwrap();
    unplaced::find(&page, t, &set, covered)
        .into_iter()
        .map(|d| {
            let value = text[d.span.start..d.span.end].to_string();
            assert!(matches!(d.source, DetectionSource::FailClosed { .. }));
            assert_eq!(d.treatment, Treatment::Tokenize);
            if d.needs_review {
                let prompt = d.review_prompt.clone().expect("a prompt");
                assert!(!prompt.contains(&value), "prompt leaks the value");
                assert!(!prompt.chars().any(|c| c.is_ascii_digit()), "{prompt}");
                assert!(prompt.ends_with("Ink it?"), "{prompt}");
            }
            (d.kind, value, d.needs_review)
        })
        .collect()
}

fn found(text: &str) -> Vec<(TokenKind, String, bool)> {
    found_with(text, &[])
}

fn values(text: &str) -> Vec<String> {
    found(text).into_iter().map(|(_, v, _)| v).collect()
}

const HEAD: &str = "Demo Statement\nCopy B\nFor Recipient\nRecipient's name\n1 Gross Amount\n$\nInternal Revenue Service\n";

#[test]
fn capitalised_runs_are_held_as_names() {
    let f = found(&format!(
        "{HEAD}Sam T Sample\nAdatum Mortgage Co\nMaria Example-Lopez\nWingtip Design LP\n"
    ));
    assert_eq!(
        f,
        vec![
            (TokenKind::Person, "Sam T Sample".into(), true),
            (TokenKind::Org, "Adatum Mortgage Co".into(), true),
            (TokenKind::Person, "Maria Example-Lopez".into(), true),
            (TokenKind::Org, "Wingtip Design LP".into(), true),
        ]
    );
}

#[test]
fn labels_titles_boilerplate_and_prose_are_not_names() {
    // The head alone: the anchor, labels and boilerplate lines.
    assert!(found(HEAD).is_empty());
    for line in [
        "Miscellaneous",
        "penalty or other",
        "Wage and Tax Statement",
        "One Two Three Four Five",
        "Sam   12,000.00",
        "Q R",
        "PAYER’S TIN   RECIPIENT’S TIN",
        // Boilerplate after a label on the same line.
        "1 Gross Amount  Beginning  Ending",
    ] {
        assert!(values(&format!("{HEAD}{line}\n")).is_empty(), "{line}");
    }
}

#[test]
fn codes_and_long_numbers_are_held() {
    let f = values(&format!(
        "{HEAD}AD-4410-22\nCW-0042-77\nIL 0047-91234\nNEC-000417\nref 1234567\n"
    ));
    assert_eq!(
        f,
        [
            "AD-4410-22",
            "CW-0042-77",
            "0047-91234",
            "NEC-000417",
            "1234567"
        ]
    );
    let kinds: Vec<TokenKind> = found(&format!("{HEAD}AD-4410-22\n"))
        .into_iter()
        .map(|(k, ..)| k)
        .collect();
    assert_eq!(kinds, [TokenKind::Custom("number".into())]);
}

#[test]
fn form_numbers_amounts_dates_and_codes_are_not_numbers() {
    for line in [
        "Form 1099-INT",
        "Form W-2",
        "Form 1098 (Rev. 4-2025)",
        "(Rev. January 2024)",
        "Form 1095-C",
        "OMB No. 1545-0112",
        "Cat. No. 14425J",
        "12a 13b 1E",
        "52340.18 12,000.00 $5,000",
        "03/14/2021 2025-03-14",
        "Tucson, AZ 85701",
        "85701-1234",
        "22222",
        "www.irs.gov/Form1099INT",
    ] {
        assert!(values(&format!("{HEAD}{line}\n")).is_empty(), "{line}");
    }
}

#[test]
fn an_account_code_next_to_its_label_is_tokenized_without_a_question() {
    let f = found(&format!("{HEAD}Acct no. AD-4410-22\n"));
    assert_eq!(f, vec![(TokenKind::Account, "AD-4410-22".into(), false)]);
}

#[test]
fn covered_text_is_left_alone() {
    let text = format!("{HEAD}Sam T Sample\nAD-4410-22\n");
    let at = text.find("Sam").unwrap();
    let code = text.find("AD-").unwrap();
    assert!(found_with(&text, &[(at, at + 12), (code, code + 10)]).is_empty());
}

#[test]
fn street_lines_are_tokenized_as_addresses_without_a_question() {
    let f = found(&format!(
        "{HEAD}77 Oak Ave\n1428 Elm Street Apt 4B\n500 Harbor Blvd Suite 200\n"
    ));
    assert_eq!(
        f,
        vec![
            (TokenKind::Address, "77 Oak Ave".into(), false),
            (TokenKind::Address, "1428 Elm Street Apt 4B".into(), false),
            (
                TokenKind::Address,
                "500 Harbor Blvd Suite 200".into(),
                false
            ),
        ]
    );
    // Box labels that start with a number are not streets.
    for line in [
        "12 Street address (including room or suite no.)",
        "2 Royalties",
    ] {
        assert!(values(&format!("{HEAD}{line}\n")).is_empty(), "{line}");
    }
}

#[test]
fn a_year_glued_to_a_word_is_not_a_number() {
    // pdf-extract output: "Form 1095-B 2025Department of the Treasury".
    assert!(values(&format!("{HEAD}2025Department of the Treasury\n")).is_empty());
}

#[test]
fn a_repeated_value_is_asked_about_once() {
    let f = found(&format!(
        "{HEAD}Pat R Example\nAD-4410-22\nPat R Example\nAD-4410-22\n"
    ));
    assert_eq!(
        f,
        vec![
            (TokenKind::Person, "Pat R Example".into(), true),
            (
                TokenKind::Custom("number".into()),
                "AD-4410-22".into(),
                true
            ),
            (TokenKind::Person, "Pat R Example".into(), false),
            (
                TokenKind::Custom("number".into()),
                "AD-4410-22".into(),
                false
            ),
        ]
    );
}

#[test]
fn a_code_after_a_state_is_tokenized_as_a_state_id_without_a_question() {
    let state_id = || TokenKind::Custom("state_id".into());
    let f = found(&format!(
        "{HEAD}OR 0000123-4\nNY 7700-1234\nIL   0047-91234\n"
    ));
    assert_eq!(
        f,
        vec![
            (state_id(), "0000123-4".into(), false),
            (state_id(), "7700-1234".into(), false),
            (state_id(), "0047-91234".into(), false),
        ]
    );
    // Not after a state: still a question.
    let f = found(&format!("{HEAD}ZZ 7700-1234\n"));
    assert_eq!(
        f,
        vec![(TokenKind::Custom("number".into()), "7700-1234".into(), true)]
    );
}

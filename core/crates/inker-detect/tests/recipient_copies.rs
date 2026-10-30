//! Recipient copies, statements and letters as a real tax folder has them:
//! the holder's name in capitals, surname first, or with no label at all
//! above the mailing lines, and addresses in capitals, on one line, with a
//! PO box or with the unit on a line of its own. A user's 1099-style copy and
//! a 2026 letter left the full name and street address visible
//! (2026-10-30). Every value here is synthetic: Jordan A Example, invented
//! streets, ZIPs used only as shapes.

#![allow(clippy::unwrap_used, clippy::expect_used)] // tests may panic

use std::path::PathBuf;
use std::sync::LazyLock;

use inker_core::{
    Detection, DetectionSource, Extracted, PageText, TextOrigin, TokenKind, Treatment,
};
use inker_detect::detect;
use inker_packs::PackSet;

static PACKS: LazyLock<PackSet> = LazyLock::new(|| {
    PackSet::load(Some(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../packs/tax"),
    ))
    .unwrap()
});

fn fixture(name: &str) -> String {
    std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name),
    )
    .unwrap()
}

fn run(text: &str) -> Vec<Detection> {
    let e = Extracted {
        pages: vec![PageText {
            page: 1,
            text: text.into(),
            origin: TextOrigin::TextLayer,
        }],
        fields: vec![],
    };
    detect(&e, &PACKS).unwrap().0
}

/// The detection that wholly covers the first `v` at or after `from`.
fn covering<'a>(text: &str, dets: &'a [Detection], v: &str, from: usize) -> &'a Detection {
    let at = from
        + text[from..]
            .find(v)
            .unwrap_or_else(|| panic!("{v:?} not in text"));
    let end = at + v.len();
    dets.iter()
        .find(|d| d.field.is_none() && d.span.start <= at && end <= d.span.end)
        .unwrap_or_else(|| {
            panic!(
                "{v:?} is not inked; have {:?}",
                dets.iter()
                    .map(|d| (&text[d.span.start..d.span.end], &d.kind))
                    .collect::<Vec<_>>()
            )
        })
}

/// What a reader sees: tokens for tokenized spans, "City, ST" for
/// generalized addresses.
fn visible(text: &str, dets: &[Detection]) -> String {
    let mut on: Vec<&Detection> = dets.iter().filter(|d| d.field.is_none()).collect();
    on.sort_by_key(|d| d.span.start);
    let mut out = String::new();
    let mut pos = 0;
    for d in on {
        if d.span.start < pos || d.treatment == Treatment::Keep {
            continue;
        }
        out.push_str(&text[pos..d.span.start]);
        match (&d.treatment, &d.generalized) {
            (Treatment::Generalize, Some(g)) if !d.needs_review => out.push_str(g),
            _ => out.push_str("[INKED]"),
        }
        pos = d.span.end;
    }
    out.push_str(&text[pos..]);
    out
}

/// `name` is a Person; each of `lines` is inside an Address; and none of
/// the name's words, the street or the ZIP is visible afterwards.
fn check(text: &str, name: &str, lines: &[&str]) {
    let dets = run(text);
    let from = text
        .find(name)
        .unwrap_or_else(|| panic!("{name:?} not in text"));
    let d = covering(text, &dets, name, 0);
    assert_eq!(d.kind, TokenKind::Person, "{name:?} in {text:?}");
    for l in lines {
        let d = covering(text, &dets, l, from);
        assert_eq!(d.kind, TokenKind::Address, "{l:?} in {text:?}");
        assert_eq!(d.treatment, Treatment::Generalize, "{l:?}");
    }
    // A repeat of the name in prose ("Dear ...") is inker-core's to sweep
    // once the first occurrence is inked (tokenize.rs), so only the text
    // before a salutation is checked here.
    let seen = visible(text, &dets).to_ascii_lowercase();
    let seen = seen.split("\ndear ").next().unwrap_or_default();
    for word in [
        "jordan",
        "example blvd",
        "sample street",
        "77002",
        "box 4471",
    ] {
        assert!(!seen.contains(word), "{word:?} visible in {seen:?}");
    }
}

#[test]
fn all_caps_name_heading_all_caps_address() {
    check(
        "Northwind Brokerage Services\nStatement period January 1 - March 31, 2026\n\n\
         JORDAN A EXAMPLE\n1234 EXAMPLE BLVD APT 5\nHOUSTON TX 77002-1234\n\n\
         Account Number ****1234\n",
        "JORDAN A EXAMPLE",
        &["1234 EXAMPLE BLVD APT 5", "HOUSTON TX 77002-1234"],
    );
}

#[test]
fn surname_first_name_heading_an_address() {
    check(
        "Northwind Brokerage Services\n\nEXAMPLE, JORDAN A\n1234 Example Blvd Apt 5\nHouston, TX 77002\n",
        "EXAMPLE, JORDAN A",
        &["1234 Example Blvd Apt 5", "Houston, TX 77002"],
    );
}

#[test]
fn surname_first_name_after_a_label() {
    for label in ["Account holder:", "RECIPIENT'S name:", "Recipient's name"] {
        let text = format!("{label} EXAMPLE, JORDAN A\n");
        let dets = run(&text);
        let d = covering(&text, &dets, "EXAMPLE, JORDAN A", 0);
        assert_eq!(d.kind, TokenKind::Person, "{text:?}");
    }
}

#[test]
fn unlabelled_name_at_the_top_of_a_letter() {
    check(
        "October 2, 2026\n\nJordan A Example\n1234 Example Blvd Apt 5\nHouston, TX 77002\n\n\
         Dear Jordan A Example,\n\nYour annual escrow review is enclosed.\n",
        "Jordan A Example",
        &["1234 Example Blvd Apt 5", "Houston, TX 77002"],
    );
}

#[test]
fn label_on_its_own_line_with_values_underneath() {
    check(
        "RECIPIENT'S name, street address, city or town, state or province, country, ZIP or foreign postal code\n\
         JORDAN A EXAMPLE\n1234 EXAMPLE BLVD APT 5\nHOUSTON TX 77002-1234\n",
        "JORDAN A EXAMPLE",
        &["1234 EXAMPLE BLVD APT 5", "HOUSTON TX 77002-1234"],
    );
    // The IRS text layer wraps the label before the values.
    check(
        "RECIPIENT’S name, street address, city or town, state or province, country, ZIP \n\
         or foreign postal code\nJORDAN A EXAMPLE\n1234 EXAMPLE BLVD APT 5\nHOUSTON TX 77002-1234\n",
        "JORDAN A EXAMPLE",
        &["1234 EXAMPLE BLVD APT 5", "HOUSTON TX 77002-1234"],
    );
}

#[test]
fn whole_address_on_one_line_in_capitals() {
    check(
        "Recipient's name: JORDAN A EXAMPLE\nAddress: 1234 EXAMPLE BLVD APT 5, HOUSTON, TX 77002\n",
        "JORDAN A EXAMPLE",
        &["1234 EXAMPLE BLVD APT 5, HOUSTON, TX 77002"],
    );
}

#[test]
fn po_box_in_capitals() {
    check(
        "JORDAN A EXAMPLE\nPO BOX 4471\nHOUSTON TX 77002\n",
        "JORDAN A EXAMPLE",
        &["PO BOX 4471", "HOUSTON TX 77002"],
    );
}

#[test]
fn unit_on_a_line_of_its_own() {
    check(
        "Jordan A Example\n1234 Sample Street\nApt 5\nHouston, TX 77002\n",
        "Jordan A Example",
        &["1234 Sample Street", "Apt 5", "Houston, TX 77002"],
    );
    check(
        "JORDAN A EXAMPLE\n1234 SAMPLE STREET\nUNIT 5\nHOUSTON TX 77002\n",
        "JORDAN A EXAMPLE",
        &["1234 SAMPLE STREET", "UNIT 5", "HOUSTON TX 77002"],
    );
}

#[test]
fn brokerage_recipient_copy_2025() {
    let text = fixture("brokerage-1099-2025.txt");
    check(
        &text,
        "JORDAN A EXAMPLE",
        &["1234 EXAMPLE BLVD APT 5", "HOUSTON TX 77002-1234"],
    );
}

#[test]
fn bank_letter_2026() {
    let text = fixture("letter-2026.txt");
    check(
        &text,
        "Jordan A Example",
        &["1234 Example Blvd Apt 5", "Houston, TX 77002"],
    );
}

#[test]
fn boilerplate_is_not_a_person() {
    // Headings that sit where a holder's name would, directly above the
    // mailing lines, are not people. (Fail-closed holds on a form page are
    // a separate stage with its own review prompt; only rule-found people
    // count here.)
    for heading in [
        "INTERNAL REVENUE SERVICE",
        "DEPARTMENT OF THE TREASURY",
        "Department of the Treasury - Internal Revenue Service",
        "FORM 1099-DIV DIVIDENDS AND DISTRIBUTIONS",
        "COPY B FOR RECIPIENT",
        "ORDINARY DIVIDENDS",
        "Qualified Dividends",
        "RETURN SERVICE REQUESTED",
        "IMPORTANT TAX DOCUMENT",
        "CONSOLIDATED FORM 1099",
        "PRESORTED FIRST CLASS MAIL",
    ] {
        for address in [
            "1234 Example Blvd Apt 5\nHouston, TX 77002\n",
            "1234 EXAMPLE BLVD APT 5\nHOUSTON TX 77002\n",
            "PO BOX 4471\nHOUSTON TX 77002\n",
        ] {
            let text = format!("{heading}\n{address}");
            let dets = run(&text);
            let people: Vec<_> = dets
                .iter()
                .filter(|d| {
                    d.kind == TokenKind::Person && matches!(d.source, DetectionSource::Rule { .. })
                })
                .map(|d| &text[d.span.start..d.span.end])
                .collect();
            assert!(people.is_empty(), "{text:?}: {people:?}");
        }
    }
}

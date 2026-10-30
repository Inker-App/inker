#![allow(clippy::unwrap_used, clippy::expect_used)] // tests may panic

use inker_core::{DetectionSource, PageText, TextOrigin, TokenKind, Treatment};
use inker_detect::names;

fn found(text: &str) -> Vec<(TokenKind, String, Option<String>)> {
    let page = PageText {
        page: 1,
        text: text.into(),
        origin: TextOrigin::TextLayer,
    };
    names::find(&page)
        .into_iter()
        .map(|d| {
            assert_eq!(d.treatment, Treatment::Tokenize);
            assert!(matches!(d.source, DetectionSource::Rule { .. }));
            assert!(d.confidence < 0.7, "names are low confidence");
            (d.kind, text[d.span.start..d.span.end].to_string(), d.role)
        })
        .collect()
}

fn one(
    kind: TokenKind,
    value: &str,
    role: Option<&str>,
) -> Vec<(TokenKind, String, Option<String>)> {
    vec![(kind, value.to_string(), role.map(String::from))]
}

#[test]
fn labelled_people() {
    use TokenKind::Person;
    let cases = [
        (
            "Employee's name: Jane Q. Doe",
            "Jane Q. Doe",
            Some("employee"),
        ),
        (
            "Recipient's name\nJOHN A SAMPLE\n",
            "JOHN A SAMPLE",
            Some("recipient"),
        ),
        (
            "RECIPIENT’S name: Pat Example",
            "Pat Example",
            Some("recipient"),
        ),
        (
            "Borrower: Maria Example-Lopez, 12 Main St",
            "Maria Example-Lopez",
            Some("borrower"),
        ),
        ("Prepared for: Alex Sample", "Alex Sample", None),
        ("Account holder   Chris Mock   Page 1", "Chris Mock", None),
        ("Seller: Sam T Sample", "Sam T Sample", None),
        ("Buyer: Robin Example", "Robin Example", None),
    ];
    for (t, v, role) in cases {
        assert_eq!(found(t), one(Person, v, role), "{t}");
    }
}

#[test]
fn labelled_orgs() {
    use TokenKind::Org;
    let cases = [
        (
            "Payer's name: Northwind Credit Union",
            "Northwind Credit Union",
            Some("payer"),
        ),
        (
            "Payer's name: Fabrikam Holdings",
            "Fabrikam Holdings",
            Some("payer"),
        ),
        (
            "Employer's name: Contoso Widgets LLC",
            "Contoso Widgets LLC",
            Some("employer"),
        ),
        ("Prepared for: Acme Inc.", "Acme Inc.", None),
        (
            "Lender: Woodgrove Bank, N.A.",
            "Woodgrove Bank, N.A.",
            Some("lender"),
        ),
        (
            "PAYER'S name, street address, city or town, state\nTailspin Toys Co.\n12 Main St",
            "Tailspin Toys Co.",
            Some("payer"),
        ),
    ];
    for (t, v, role) in cases {
        assert_eq!(found(t), one(Org, v, role), "{t}");
    }
}

#[test]
fn unlabelled_orgs_need_a_suffix() {
    let f = found("Northwind Bank\nStatement period: Jan 1 - Jan 31");
    assert_eq!(f, one(TokenKind::Org, "Northwind Bank", None));
    let f = found("Your Statement Northwind Federal Credit Union");
    assert_eq!(
        f,
        one(TokenKind::Org, "Northwind Federal Credit Union", None)
    );
    assert!(found("Jane Doe went home").is_empty());
    // Partnerships (Schedule K-1) and limited companies.
    for org in [
        "Coho Partners LP",
        "Coho Partners L.P.",
        "Fourth Coffee Ltd",
    ] {
        assert_eq!(
            found(&format!("{org}\n")),
            one(TokenKind::Org, org, None),
            "{org}"
        );
    }
}

#[test]
fn stop_list_and_shape_rejects() {
    for t in [
        "Employee's name: January Statement",
        "Recipient's name: New York",
        "Prepared for: Form W-2",
        "Borrower: Maria",
        "Prepared for: Alpha Beta Gamma Delta Epsilon",
        "Recipient's name: see instructions",
        "Payer's name:                                                  Far Away Name",
    ] {
        assert!(found(t).is_empty(), "{t}: {:?}", found(t));
    }
}

#[test]
fn stop_words() {
    for w in [
        "January",
        "jan",
        "Form",
        "STATEMENT",
        "Employee's",
        "Total",
        "Box",
    ] {
        assert!(names::is_stop_word(w), "{w}");
    }
    for w in ["Jane", "Doe", "Northwind"] {
        assert!(!names::is_stop_word(w), "{w}");
    }
}

#[test]
fn name_after_reports_spans() {
    let t = "Name: Jane Doe   12,000.00";
    let r = names::name_after(t, 4, 40).unwrap();
    assert_eq!(&t[r.start..r.end], "Jane Doe");
    assert!(!r.org);
    let t = "Name:\n\n  Contoso Widgets LLC\n";
    let r = names::name_after(t, 5, 40).unwrap();
    assert_eq!(&t[r.start..r.end], "Contoso Widgets LLC");
    assert!(r.org);
    assert!(names::name_after("Name: 12 Main St", 4, 40).is_none());
}

#[test]
fn run_at_reads_exactly_at_the_start() {
    let t = "x Jane Doe";
    let r = names::run_at(t, 2).unwrap();
    assert_eq!(&t[r.start..r.end], "Jane Doe");
    assert!(names::run_at(t, 0).is_none());
    assert!(
        names::run_at("New Jersey", 0).is_none(),
        "a state is not a name"
    );
}

#[test]
fn a_name_heading_a_mailing_address_is_a_person() {
    // A statement's mailing block has no label: the name line sits directly
    // above a street line. The bank statement sample leaked its holder's
    // full name this way (first end-to-end run, 2026-10-28).
    use TokenKind::Person;
    let text = "Harbor Street Credit Union\n\
                9 Harbor Street, Portland, OR 97209   (503) 555-0142\n\
                Member Statement   March 1, 2025 - March 31, 2025\n\
                \n\
                Jordan A Example\n\
                48 Lantern Way Apt 3\n\
                Springfield, OR 97477\n";
    let people: Vec<_> = found(text)
        .into_iter()
        .filter(|(k, _, _)| *k == Person)
        .collect();
    assert_eq!(people, one(Person, "Jordan A Example", None));

    // Not a name: a heading above a street line, or a line with more on it.
    for text in [
        "Member Statement\n48 Lantern Way Apt 3\n",
        "Pay To Jordan A Example For Services\n48 Lantern Way Apt 3\n",
        "Jordan A Example\nThank you for banking with us\n",
    ] {
        assert!(
            !found(text).iter().any(|(k, _, _)| *k == Person),
            "{text:?}: {:?}",
            found(text)
        );
    }
}

#[test]
fn an_org_suffix_needs_a_name_before_it() {
    // IRS instructions talk about LLCs; an article and a suffix is no name.
    for t in [
        "An LLC that is a disregarded entity should check the box",
        "If the LLC is classified as a corporation",
        "Each LLC must enter its classification",
        "A Corporation. Enter the code",
        "LLC\n",
    ] {
        assert!(found(t).is_empty(), "{t}: {:?}", found(t));
    }
    // A real name after an article is still found, without the article.
    assert_eq!(
        found("Paid to Bluefin Analytics LLC today"),
        one(TokenKind::Org, "Bluefin Analytics LLC", None)
    );
    assert_eq!(
        found("An Acme Robotics Inc invoice"),
        one(TokenKind::Org, "Acme Robotics Inc", None)
    );
}

#[test]
fn agencies_are_never_people() {
    for w in [
        "IRS",
        "SSA",
        "TIGTA",
        "FTC",
        "Taxpayer",
        "Advocate",
        "Administration",
    ] {
        assert!(names::is_stop_word(w), "{w}");
    }
    for t in [
        "Recipient's name: Internal Revenue Service",
        "Recipient's name: IRS Office",
        "Recipient's name: Social Security Administration",
        "Recipient's name: Taxpayer Advocate Service",
        "Employee's name: SSA Field Office",
    ] {
        assert!(found(t).is_empty(), "{t}: {:?}", found(t));
    }
}

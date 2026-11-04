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
fn surname_first_names() {
    use TokenKind::Person;
    for (t, v) in [
        ("Account holder: EXAMPLE, JORDAN A", "EXAMPLE, JORDAN A"),
        ("Recipient's name: Example, Jordan A", "Example, Jordan A"),
        ("Borrower: SAMPLE, RILEY\n", "SAMPLE, RILEY"),
    ] {
        let f: Vec<_> = found(t)
            .into_iter()
            .filter(|(k, _, _)| *k == Person)
            .collect();
        assert_eq!(f.len(), 1, "{t}: {f:?}");
        assert_eq!(f[0].1, v, "{t}");
    }
    let t = "x EXAMPLE, JORDAN A";
    let r = names::run_at(t, 2).unwrap();
    assert_eq!(&t[r.start..r.end], "EXAMPLE, JORDAN A");
    // A city and state is not a surname and a given name.
    for t in [
        "Houston, TX",
        "Austin, Texas",
        "Springfield, New York",
        "Total, Federal Tax",
    ] {
        assert!(names::run_at(t, 0).is_none(), "{t}");
    }
}

#[test]
fn a_name_in_capitals_heading_a_mailing_block() {
    use TokenKind::Person;
    for (text, name) in [
        (
            "JORDAN A EXAMPLE\n1234 EXAMPLE BLVD APT 5\nHOUSTON TX 77002\n",
            "JORDAN A EXAMPLE",
        ),
        (
            "JORDAN A EXAMPLE\nPO BOX 4471\nHOUSTON TX 77002\n",
            "JORDAN A EXAMPLE",
        ),
        (
            "EXAMPLE, JORDAN A\n1234 Example Blvd Apt 5\nHouston, TX 77002\n",
            "EXAMPLE, JORDAN A",
        ),
    ] {
        let people: Vec<_> = found(text)
            .into_iter()
            .filter(|(k, _, _)| *k == Person)
            .collect();
        assert_eq!(people, one(Person, name, None), "{text:?}");
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

// Letters and agreements (2026-11-04). Claude reported an addressee after
// "Dear", a signatory over a title and four lawyers in a notices block all
// left in plain text. Every name below is invented.

fn people(text: &str) -> Vec<(String, Option<String>)> {
    found(text)
        .into_iter()
        .filter(|(k, _, _)| *k == TokenKind::Person)
        .map(|(_, v, r)| (v, r))
        .collect()
}

fn person(v: &str, role: Option<&str>) -> (String, Option<String>) {
    (v.to_string(), role.map(String::from))
}

#[test]
fn the_name_after_dear_is_the_addressee() {
    for (t, v) in [
        ("Dear Odalys Fenwick,\n\nThank you.", "Odalys Fenwick"),
        ("Dear Ms. Fenwick:\n", "Ms. Fenwick"),
        ("Dear Dr. Odalys Fenwick,\n", "Dr. Odalys Fenwick"),
        ("   Dear Tobiah Vance-Mercer,\n", "Tobiah Vance-Mercer"),
        ("Dear Odalys,\n", "Odalys"),
        ("DEAR ODALYS FENWICK:\n", "ODALYS FENWICK"),
    ] {
        assert_eq!(people(t), vec![person(v, Some("addressee"))], "{t:?}");
    }
}

#[test]
fn generic_salutations_are_nobody() {
    for t in [
        "Dear Sir or Madam,\n",
        "Dear Sir,\n",
        "Dear Madam:\n",
        "Dear Customer,\n",
        "Dear Valued Client,\n",
        "Dear Valued Customer,\n",
        "To Whom It May Concern:\n",
        "Dear Team,\n",
        "Dear Colleagues,\n",
        "Dear Shareholders,\n",
        "Dear Member,\n",
        "Dear Friends,\n",
        "Dear All,\n",
        "Dear Hiring Manager,\n",
        "Dear Board of Directors,\n",
        "Dear Sincerely,\n",
        "Dear January,\n",
        "dear odalys fenwick,\n",
    ] {
        assert!(people(t).is_empty(), "{t:?}: {:?}", people(t));
    }
}

#[test]
fn the_name_under_a_closing_is_the_signatory() {
    for closing in [
        "Sincerely,",
        "Sincerely yours,",
        "Best regards,",
        "Kind regards,",
        "Regards,",
        "Yours truly,",
        "Very truly yours,",
        "Respectfully,",
    ] {
        let t = format!("Thanks again.\n\n{closing}\n\n\nThaddeus Wrenfield\n");
        assert_eq!(
            people(&t),
            vec![person("Thaddeus Wrenfield", Some("signatory"))],
            "{t:?}"
        );
    }
    // A company line or a signature rule may sit between them.
    let t =
        "Very truly yours,\nBrackenridge Copperworks, Inc.\n\n______________\nThaddeus Wrenfield\n";
    assert_eq!(
        people(t),
        vec![person("Thaddeus Wrenfield", Some("signatory"))]
    );
}

#[test]
fn what_follows_a_closing_but_is_no_name_is_left() {
    for t in [
        "Sincerely,\n\nMember Services\n",
        "Sincerely,\n\nThe Board of Directors\n",
        "Sincerely,\n\nChief Executive Officer\n",
        "Regards,\nBrackenridge Copperworks, Inc.\n",
        "Sincerely,\n\nOctober 14, 2026\n",
        "Best regards,\n\nCustomer Care Team\n",
    ] {
        assert!(people(t).is_empty(), "{t:?}: {:?}", people(t));
    }
}

#[test]
fn a_name_over_a_title_is_the_signatory() {
    for title in [
        "Chief Executive Officer",
        "General Counsel",
        "Partner",
        "President",
        "Director",
        "Vice President of Finance",
        "Director of Procurement",
        "Managing Director",
        "Chief Financial Officer and Treasurer",
    ] {
        let t = format!("Imelda Corvane\n{title}\nSablewood Marine Holdings LLC\n");
        assert_eq!(
            people(&t),
            vec![person("Imelda Corvane", Some("signatory"))],
            "{t:?}"
        );
    }
    // An honorific stays with the name.
    assert_eq!(
        people("Ms. Odalys Fenwick\nDirector of Procurement\n"),
        vec![person("Ms. Odalys Fenwick", Some("signatory"))]
    );
}

#[test]
fn titles_and_headings_over_titles_are_nobody() {
    for t in [
        "Chief Executive Officer\nGeneral Counsel\n",
        "Vice President\nPresident\n",
        "Board of Directors\nPresident\n",
        "Notices\nDirector\n",
        "Imelda Corvane went to Seattle\nPresident\n",
        "Imelda Corvane\nThe president will call you.\n",
        "Sablewood Marine Holdings LLC\nPresident\n",
    ] {
        assert!(people(t).is_empty(), "{t:?}: {:?}", people(t));
    }
}

#[test]
fn signature_lines_name_the_signatory() {
    for (t, v) in [
        (
            "Name: Thorvald Penhallow\nTitle: Chief Executive Officer\n",
            "Thorvald Penhallow",
        ),
        (
            "By: ________________\nName: Seraphina Oyelaran-Bix\n",
            "Seraphina Oyelaran-Bix",
        ),
        ("By: Thorvald Penhallow\n", "Thorvald Penhallow"),
        ("By: /s/ Thorvald Penhallow\n", "Thorvald Penhallow"),
        ("/s/ Ingrid Solvei Marchetti\n", "Ingrid Solvei Marchetti"),
        (
            "Signature: /s/ Ingrid Solvei Marchetti   Date: 9/30/2026\n",
            "Ingrid Solvei Marchetti",
        ),
    ] {
        assert_eq!(people(t), vec![person(v, Some("signatory"))], "{t:?}");
    }
    // A Title: line, a blank By: line and a lone "Name:" are not names.
    for t in [
        "Title: Chief Executive Officer\n",
        "By: ______________________\n",
        "Name:\nTitle:\n",
        "By: Kestrel Ridge Analytics, Inc.\n",
        "Name: Chief Executive Officer\nTitle: President\n",
    ] {
        assert!(people(t).is_empty(), "{t:?}: {:?}", people(t));
    }
}

#[test]
fn notice_and_attention_names_are_contacts() {
    for (t, v) in [
        (
            "Attention: Wendeline Okoro, General Counsel\n",
            "Wendeline Okoro",
        ),
        ("Attn: Wendeline Okoro\n", "Wendeline Okoro"),
        ("ATTN: WENDELINE OKORO\n", "WENDELINE OKORO"),
        ("c/o Wendeline Okoro\n12 Main St\n", "Wendeline Okoro"),
        (
            "with a copy to: Desmond Achterberg, Ravensholm Abernethy LLP\n",
            "Desmond Achterberg",
        ),
        (
            "with a copy to:\nDesmond Achterberg, Ravensholm Abernethy LLP\n",
            "Desmond Achterberg",
        ),
        (
            "           Marisol Teague-Lindqvist, Esq.\n",
            "Marisol Teague-Lindqvist",
        ),
        ("Fitzgerald Amberley, Esq.\n", "Fitzgerald Amberley"),
    ] {
        assert_eq!(people(t), vec![person(v, Some("contact"))], "{t:?}");
    }
    // Both lines of a two-lawyer attention block.
    let t = "Attention: Fitzgerald Amberley, Esq.\n           Marisol Teague-Lindqvist, Esq.\n";
    assert_eq!(
        people(t),
        vec![
            person("Fitzgerald Amberley", Some("contact")),
            person("Marisol Teague-Lindqvist", Some("contact")),
        ]
    );
    // The firm in "Name, Firm LLP" is an organisation, not a person.
    let f = found("with a copy to: Desmond Achterberg, Ravensholm Abernethy LLP\n");
    assert!(
        f.iter()
            .any(|(k, v, _)| *k == TokenKind::Org && v == "Ravensholm Abernethy LLP"),
        "{f:?}"
    );
}

#[test]
fn attention_to_a_role_or_firm_is_nobody() {
    for t in [
        "Attention: General Counsel\n",
        "Attention: Legal Department\n",
        "Attn: Chief Financial Officer\n",
        "with a copy to:\nPembury Stroud LLP\n",
        "with a copy (which shall not constitute notice) to:\n",
        "Notices, Esq.\n",
    ] {
        assert!(people(t).is_empty(), "{t:?}: {:?}", people(t));
    }
}

#[test]
fn cc_lines_name_contacts() {
    let t = "cc: Lucan Ashgrove, Vice President of Finance\n    Imelda Corvane, General Counsel\n";
    assert_eq!(
        people(t),
        vec![
            person("Lucan Ashgrove", Some("contact")),
            person("Imelda Corvane", Some("contact")),
        ]
    );
    assert_eq!(
        people("cc: Lucan Ashgrove; Imelda Corvane\n"),
        vec![
            person("Lucan Ashgrove", Some("contact")),
            person("Imelda Corvane", Some("contact")),
        ]
    );
    for t in ["cc: File\n", "cc: Board of Directors\n", "Accent: none\n"] {
        assert!(people(t).is_empty(), "{t:?}: {:?}", people(t));
    }
}

#[test]
fn a_name_defining_a_party_is_a_party() {
    for (t, v) in [
        (
            "and Ingrid Solvei Marchetti (the \"Holder\").",
            "Ingrid Solvei Marchetti",
        ),
        (
            "between Ingrid Marchetti (\"Executive\") and",
            "Ingrid Marchetti",
        ),
        (
            "Ingrid Marchetti, an individual (the “Holder”)",
            "Ingrid Marchetti",
        ),
    ] {
        assert_eq!(people(t), vec![person(v, Some("party"))], "{t:?}");
    }
    // A company defining itself is an organisation; a defined term alone is
    // no one.
    for t in [
        "Kestrel Ridge Analytics, Inc., a Delaware corporation (the \"Company\")",
        "This Option Cancellation Agreement (this \"Agreement\")",
        "the Merger Agreement (the \"Merger Agreement\")",
        "Plan of Merger (the \"Plan\")",
    ] {
        assert!(people(t).is_empty(), "{t:?}: {:?}", people(t));
    }
}

#[test]
fn an_honorific_marks_a_person_in_prose() {
    for (t, v) in [
        (
            "should go to Ms. Perpetua Quayle at the bank",
            "Ms. Perpetua Quayle",
        ),
        ("as Mr. Quayle explained", "Mr. Quayle"),
        (
            "Dr. Perpetua Quayle-Nunn will attend",
            "Dr. Perpetua Quayle-Nunn",
        ),
        ("ask Mrs. Quayle.", "Mrs. Quayle"),
    ] {
        assert_eq!(people(t), vec![person(v, None)], "{t:?}");
    }
    for t in [
        "Dr. Martin Luther King Jr. Blvd",
        "Mr. and Mrs.",
        "Ms.",
        "the Dr. said",
        // "Dr." for Drive after a street name is no honorific.
        "77 Harbor Light Dr. Suite 210",
        "Elm Dr. North",
    ] {
        assert!(people(t).is_empty(), "{t:?}: {:?}", people(t));
    }
}

#[test]
fn organisation_names_in_capitals_are_organisations() {
    // Signature pages print the party's name in capitals.
    for org in [
        "KESTREL RIDGE ANALYTICS, INC.",
        "HALVORSEN PEAK SYSTEMS CORP.",
        "PEMBURY STROUD LLP",
    ] {
        let t = format!("{org}\n\nBy: ________\n");
        assert_eq!(found(&t), one(TokenKind::Org, org, None), "{t:?}");
    }
}

#[test]
fn a_cc_block_flattened_by_ocr_still_names_its_contacts() {
    let t = "cc: Lucan Ashgrove, Vice President of Finance\nImelda Corvane, General Counsel\n\nEnclosure\nPortland Harbor Tours\n";
    assert_eq!(
        people(t),
        vec![
            person("Lucan Ashgrove", Some("contact")),
            person("Imelda Corvane", Some("contact")),
        ]
    );
}

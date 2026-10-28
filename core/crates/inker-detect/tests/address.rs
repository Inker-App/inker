#![allow(clippy::unwrap_used, clippy::expect_used)] // tests may panic

use inker_core::{DetectionSource, PageText, TextOrigin, TokenKind, Treatment};
use inker_detect::address;

fn found(text: &str) -> Vec<(String, Option<String>)> {
    let page = PageText {
        page: 1,
        text: text.into(),
        origin: TextOrigin::TextLayer,
    };
    address::find(&page)
        .into_iter()
        .map(|d| {
            assert_eq!(d.kind, TokenKind::Address);
            assert_eq!(d.treatment, Treatment::Generalize);
            assert!(matches!(d.source, DetectionSource::Rule { .. }));
            (text[d.span.start..d.span.end].to_string(), d.generalized)
        })
        .collect()
}

#[test]
fn full_addresses_generalize_to_city_and_state() {
    let cases = [
        ("1428 Elm Street\nBrooklyn, NY 11201", "Brooklyn, NY"),
        (
            "1428 Elm Street, Apt 4B, Brooklyn, NY 11201-1234",
            "Brooklyn, NY",
        ),
        (
            "77 N. Harbor Blvd Suite 300\nSan Jose, CA 95112",
            "San Jose, CA",
        ),
        ("PO Box 4471\nAustin, TX 78701", "Austin, TX"),
        (
            "9 Orchard Ln # 12\n  Saint Paul, MN 55101",
            "Saint Paul, MN",
        ),
        ("300 W 57th St Fl 9, New York, NY 10019", "New York, NY"),
    ];
    for (text, city) in cases {
        let f = found(&format!("Mail to:\n{text}\nThank you."));
        assert_eq!(
            f,
            vec![(text.to_string(), Some(city.to_string()))],
            "{text}"
        );
    }
}

#[test]
fn bare_city_state_zip_drops_the_zip() {
    let f = found("Statement for\nSpringfield, IL 62701\nPage 1");
    assert_eq!(
        f,
        vec![(
            "Springfield, IL 62701".to_string(),
            Some("Springfield, IL".to_string())
        )]
    );
}

#[test]
fn several_addresses_on_a_page() {
    let t = "Payer\n12 Main St\nDenver, CO 80202\n\nRecipient\n5 Oak Ave\nBoise, ID 83702\n";
    let f = found(t);
    assert_eq!(f.len(), 2);
    assert_eq!(f[0].1.as_deref(), Some("Denver, CO"));
    assert_eq!(f[1].1.as_deref(), Some("Boise, ID"));
}

#[test]
fn rejects_non_addresses() {
    for t in [
        "Brooklyn, NY",                      // already general
        "Wages, XX 12345",                   // not a state
        "1 Wages, tips, other compensation", // a box label
        "12 Code D 1234.00",                 // a box value
        "Total, NY 1234",                    // four-digit ZIP
        "Paid 2025 Interest, NY 112011",     // six digits
    ] {
        assert!(found(t).is_empty(), "{t}");
    }
}

#[test]
fn generalize_reads_the_city_line() {
    assert_eq!(
        address::generalize("12 Main St\nDenver, CO 80202").as_deref(),
        Some("Denver, CO")
    );
    assert_eq!(
        address::generalize("Denver CO 80202").as_deref(),
        Some("Denver, CO")
    );
    assert_eq!(address::generalize("12 Main St"), None);
}

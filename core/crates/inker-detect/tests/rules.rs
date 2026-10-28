//! Table-driven accept/reject tests for every rule.
//!
//! No real identifiers appear here. SSN literals use area 000, 666 or 9xx (or
//! are invalid for another stated reason); EIN literals use unissued prefixes;
//! routing literals are synthetic (checksum-valid ones were constructed for
//! these tests). Where a test needs a value that passes the real-world SSN or
//! EIN check, it is built at runtime from its parts so no such literal is
//! committed.

#![allow(clippy::unwrap_used, clippy::expect_used)] // tests may panic

use inker_core::{DetectionSource, PageText, TextOrigin, TokenKind, Treatment};
use inker_detect::rules::{self, SsnCheck};

fn page(text: &str) -> PageText {
    PageText {
        page: 1,
        text: text.into(),
        origin: TextOrigin::TextLayer,
    }
}

/// (kind, matched text, confidence) for every rule detection in `text`.
fn found(text: &str) -> Vec<(TokenKind, String, f32)> {
    let mut v: Vec<_> = rules::find(&page(text))
        .into_iter()
        .map(|d| {
            assert_eq!(d.treatment, Treatment::Tokenize);
            assert!(matches!(d.source, DetectionSource::Rule { .. }));
            assert!(!d.needs_review);
            (
                d.kind,
                text[d.span.start..d.span.end].to_string(),
                d.confidence,
            )
        })
        .collect();
    v.sort_by_key(|(_, s, _)| text.find(s.as_str()));
    v
}

fn has(text: &str, kind: TokenKind, value: &str) -> bool {
    found(text).iter().any(|(k, v, _)| *k == kind && v == value)
}

fn none_of(text: &str, kind: TokenKind) -> bool {
    !found(text).iter().any(|(k, _, _)| *k == kind)
}

// ---------------------------------------------------------------- SSN

#[test]
fn ssn_validator_rejects() {
    let cases = [
        ("000-12-3456", SsnCheck::Reserved),
        ("666-12-3456", SsnCheck::Reserved),
        ("900-12-3456", SsnCheck::Reserved),
        ("999-12-3456", SsnCheck::Reserved),
        ("950-49-1234", SsnCheck::Reserved),
        ("123-00-4567", SsnCheck::Invalid), // group 00
        ("123-45-0000", SsnCheck::Invalid), // serial 0000
        ("078-05-1120", SsnCheck::Invalid), // sample
        ("219-09-9999", SsnCheck::Invalid), // sample
        ("123-45-6789", SsnCheck::Invalid), // sample
        ("12-345-6789", SsnCheck::Invalid),
        ("12345678", SsnCheck::Invalid),
        ("1234567890", SsnCheck::Invalid),
        ("abc-de-fghi", SsnCheck::Invalid),
        ("", SsnCheck::Invalid),
    ];
    for (s, want) in cases {
        assert_eq!(rules::ssn_check(s), want, "{s}");
        assert!(!rules::is_valid_ssn(s), "{s}");
    }
}

#[test]
fn ssn_validator_accepts_by_structure() {
    // Built at runtime from parts: every non-reserved area with a non-zero
    // group and serial passes. No such value is written in this file.
    for area in [1u32, 100, 500, 665, 667, 899] {
        for group in [1u32, 42, 99] {
            for serial in [1u32, 1000, 9999] {
                let dashed = format!("{area:03}-{group:02}-{serial:04}");
                let bare = dashed.replace('-', "");
                if rules::SSN_SAMPLES.contains(&bare.as_str()) {
                    continue;
                }
                assert_eq!(
                    rules::ssn_check(&dashed),
                    SsnCheck::Valid,
                    "area {area} group {group}"
                );
                assert!(rules::is_valid_ssn(&bare));
            }
        }
    }
}

#[test]
fn ssn_in_text() {
    // Reserved-area values written in SSN format are still inked (lower confidence).
    let f = found("Employee SSN: 900-14-2231 on file");
    assert_eq!(f.len(), 1);
    assert_eq!(f[0].0, TokenKind::Ssn);
    assert_eq!(f[0].1, "900-14-2231");
    assert!(f[0].2 < 0.7, "reserved range is lower confidence");
    assert!(has("ssn 666 12 3456.", TokenKind::Ssn, "666 12 3456"));
    // A valid one, built at runtime, gets high confidence.
    let valid = format!("{:03}-{:02}-{:04}", 321, 54, 9876);
    let f = found(&format!("SSN {valid}"));
    assert_eq!(f[0].0, TokenKind::Ssn);
    assert!(f[0].2 >= 0.9);
    // Rejected shapes.
    for t in [
        "ref 123-00-4567",
        "ref 123-45-0000",
        "sample 078-05-1120",
        "1234-56-7890",
        "12-345-6789",
    ] {
        assert!(none_of(t, TokenKind::Ssn), "{t}");
    }
    // Inside a longer number.
    assert!(none_of("9900-14-22311", TokenKind::Ssn));
}

#[test]
fn bare_nine_digits_need_ssn_context() {
    let f = found("Social Security Number 900142231");
    assert_eq!(f.len(), 1);
    assert_eq!(f[0].0, TokenKind::Ssn);
    assert!(f[0].2 < 0.7);
    assert!(has("SSN:900142231", TokenKind::Ssn, "900142231"));
    // No context: no rule detection (fail-closed handles it later).
    assert!(found("Reference 900142231").is_empty());
    // Too far from the label.
    let far = format!("SSN{}900142231", " filler".repeat(12));
    assert!(found(&far).is_empty());
}

// ---------------------------------------------------------------- ITIN

#[test]
fn itin_validator() {
    let accept = [
        "900-50-0001",
        "999-65-1234",
        "912-70-5555",
        "923-88-1000",
        "931-90-0420",
        "942-92-1111",
        "953-94-2222",
        "964-99-3333",
        "975801234",
    ];
    for s in accept {
        assert!(rules::is_valid_itin(s), "{s}");
        assert_eq!(rules::ssn_check(s), SsnCheck::Itin, "{s}");
    }
    let reject = [
        "900-49-0001",
        "900-66-0001",
        "900-69-0001",
        "900-89-0001",
        "900-93-0001",
        "000-70-1234",
        "666-70-1234",
        "90-070-1234",
        "9007012345",
    ];
    for s in reject {
        assert!(!rules::is_valid_itin(s), "{s}");
    }
}

#[test]
fn itin_in_text() {
    assert!(has("ITIN 912-70-5555", TokenKind::Itin, "912-70-5555"));
    assert!(has("Taxpayer id 912705555", TokenKind::Itin, "912705555"));
    assert!(none_of("ITIN 912-93-5555", TokenKind::Itin));
}

// ---------------------------------------------------------------- EIN

#[test]
fn ein_prefix_list_is_complete() {
    let issued: Vec<&str> = rules::EIN_PREFIXES.iter().map(|(p, _)| *p).collect();
    let unissued = [
        "00", "07", "08", "09", "17", "18", "19", "28", "29", "49", "69", "70", "78", "79", "89",
        "96", "97",
    ];
    for n in 0..100 {
        let p = format!("{n:02}");
        assert_eq!(
            issued.contains(&p.as_str()),
            !unissued.contains(&p.as_str()),
            "prefix {p}"
        );
    }
    assert_eq!(issued.len(), 83);
}

#[test]
fn ein_validator() {
    // Accept: every issued prefix, built at runtime.
    for (p, campus) in rules::EIN_PREFIXES {
        let v = format!("{p}-{:07}", 4400);
        assert!(rules::is_valid_ein(&v), "{v}");
        assert_eq!(rules::ein_campus(&v), Some(*campus));
        assert!(rules::is_valid_ein(&v.replace('-', "")));
    }
    let reject = [
        "00-1234567",
        "07-1234567",
        "08-1234567",
        "09-1234567",
        "17-1234567",
        "18-1234567",
        "19-1234567",
        "28-1234567",
        "29-1234567",
        "49-1234567",
        "69-1234567",
        "70-1234567",
        "78-1234567",
        "79-1234567",
        "89-1234567",
        "96-1234567",
        "97-1234567",
        "07-123456",
        "07-12345678",
        "ab-cdefghi",
    ];
    for s in reject {
        assert!(!rules::is_valid_ein(s), "{s}");
        assert_eq!(rules::ein_campus(s), None, "{s}");
    }
}

#[test]
fn ein_in_text() {
    // Unissued prefix in EIN format: inked at lower confidence.
    let f = found("Employer ID 07-4400123");
    assert_eq!(f.len(), 1);
    assert_eq!(
        (f[0].0.clone(), f[0].1.as_str()),
        (TokenKind::Ein, "07-4400123")
    );
    assert!(f[0].2 < 0.7);
    // Issued prefix, built at runtime: high confidence.
    let v = format!("{}-{:07}", "45", 4400);
    let f = found(&format!("EIN {v}"));
    assert_eq!(f[0].0, TokenKind::Ein);
    assert!(f[0].2 >= 0.9);
    // Bare nine digits near "EIN" with an issued prefix.
    let bare = v.replace('-', "");
    assert!(has(&format!("EIN: {bare}"), TokenKind::Ein, &bare));
    // Not inside a longer number or a date-like string.
    assert!(none_of("107-44001234", TokenKind::Ein));
}

// ---------------------------------------------------------------- ABA routing

#[test]
fn aba_validator() {
    // All synthetic: constructed to pass the checksum for these tests.
    let synthetic = [
        "011200446", // pii-check: allow (synthetic, constructed for the checksum test)
        "214701357", // pii-check: allow (synthetic, constructed for the checksum test)
        "619022181", // pii-check: allow (synthetic, constructed for the checksum test)
        "803305711", // pii-check: allow (synthetic, constructed for the checksum test)
    ];
    for s in synthetic {
        assert!(rules::is_valid_aba(s), "{s}");
    }
    let reject = [
        "011200447", // checksum
        "130044172", // checksum ok, prefix 13 not a Federal Reserve range
        "330102021", // prefix 33
        "730001230", // prefix 73
        "998877662", // prefix 99
        "01120044",  // 8 digits
        "0112004460",
        "01120044a",
    ];
    for s in reject {
        assert!(!rules::is_valid_aba(s), "{s}");
    }
}

#[test]
fn routing_in_text() {
    // Synthetic checksum-valid routing number.
    assert!(has(
        "Routing number: 011200446", // pii-check: allow (synthetic, constructed for the checksum test)
        TokenKind::Routing,
        "011200446" // pii-check: allow (synthetic, constructed for the checksum test)
    ));
    assert!(has("ABA 011200446", TokenKind::Routing, "011200446")); // pii-check: allow (synthetic, constructed for the checksum test)
    assert!(has(
        "wire to 011200446 ref 4471", // pii-check: allow (synthetic, constructed for the checksum test)
        TokenKind::Routing,
        "011200446" // pii-check: allow (synthetic, constructed for the checksum test)
    ));
    // Bad checksum near "routing": not a routing detection.
    assert!(none_of("Routing number: 011200447", TokenKind::Routing));
    // No context: not a routing detection.
    assert!(none_of("Reference 011200446", TokenKind::Routing)); // pii-check: allow (synthetic, constructed for the checksum test)
    // Routing context is checked before the account context.
    let f = found("Routing 011200446 Account 556677889"); // pii-check: allow (synthetic, constructed for the checksum test)
    assert_eq!(f[0].0, TokenKind::Routing);
    assert_eq!(f[1].0, TokenKind::Account);
}

// ---------------------------------------------------------------- Cards

#[test]
fn card_validator() {
    // Network test numbers, published for exactly this purpose.
    let accept = [
        ("4111111111111111", "visa"),
        ("5555555555554444", "mastercard"),
        ("2223003122003222", "mastercard"),
        ("378282246310005", "amex"),
        ("6011111111111117", "discover"),
        ("3566002020360505", "jcb"),
    ];
    for (s, brand) in accept {
        assert!(rules::luhn(s), "{s}");
        assert!(rules::is_valid_card(s), "{s}");
        assert_eq!(rules::card_brand(s), Some(brand), "{s}");
    }
    let reject = [
        "4111111111111112",     // Luhn
        "1234567812345670", // Luhn ok, no known IIN ; pii-check: allow (synthetic, constructed for the checksum test)
        "411111111111",     // 12 digits
        "41111111111111111111", // 20 digits
        "37828224631000",   // amex length 14
    ];
    for s in reject {
        assert!(!rules::is_valid_card(s), "{s}");
    }
}

#[test]
fn card_in_text() {
    assert!(has(
        "Card 4111 1111 1111 1111 exp",
        TokenKind::Card,
        "4111 1111 1111 1111"
    ));
    assert!(has(
        "card: 4111-1111-1111-1111",
        TokenKind::Card,
        "4111-1111-1111-1111"
    ));
    assert!(has(
        "amex 378282246310005",
        TokenKind::Card,
        "378282246310005"
    ));
    assert!(none_of("card 4111 1111 1111 1112", TokenKind::Card));
}

// ---------------------------------------------------------------- Email, phone

#[test]
fn email_in_text() {
    for e in [
        "jane.doe@example.com",
        "a+b@mail.example.org",
        "x_y@sub.example.co",
    ] {
        assert!(has(&format!("Contact {e}."), TokenKind::Email, e), "{e}");
    }
    for t in ["not an email@", "user@localhost", "@example.com"] {
        assert!(none_of(t, TokenKind::Email), "{t}");
    }
}

#[test]
fn phone_in_text() {
    for p in [
        "(212) 555-0142",
        "212-555-0142",
        "212.555.0142",
        "+1 212 555 0142",
        "1-212-555-0142",
    ] {
        assert!(has(&format!("Call {p} today"), TokenKind::Phone, p), "{p}");
    }
    for t in [
        "call 555-0142",
        "(112) 555-0142",
        "212-155-0142",
        "total 1,212.55",
    ] {
        assert!(none_of(t, TokenKind::Phone), "{t}");
    }
}

// ---------------------------------------------------------------- Accounts

#[test]
fn account_numbers_near_labels() {
    let cases = [
        ("Account number: 7700012345678", "7700012345678"),
        ("Acct # 44556677", "44556677"),
        ("A/C 123456", "123456"),
        ("Member number 00981234", "00981234"),
        ("Policy No. 7712-3344-55", "7712-3344-55"),
        ("Loan number 1234567890", "1234567890"),
        ("Mortgage number: 99001234567", "99001234567"),
        ("Account ending in 77001234", "77001234"),
    ];
    for (t, v) in cases {
        assert!(has(t, TokenKind::Account, v), "{t}");
    }
    let reject = [
        "Account number: 12345",        // too short
        "Account 123456789012345678",   // 18 digits
        "Reference 7700012345678",      // no label
        "Account balance 1,234,567.89", // an amount
        "Account opened 2025-01-15",    // a date
    ];
    for t in reject {
        assert!(none_of(t, TokenKind::Account), "{t}");
    }
}

#[test]
fn account_confidence_falls_with_distance() {
    let near = found("Account: 7700012345678");
    let far = found("Account (see the summary on the next page) 7700012345678");
    assert_eq!(near[0].0, TokenKind::Account);
    assert_eq!(far[0].0, TokenKind::Account);
    assert!(near[0].2 > far[0].2, "{} vs {}", near[0].2, far[0].2);
}

#[test]
fn spans_are_byte_offsets() {
    // Multi-byte characters before the value.
    let t = "Café – Account № 7700012345678";
    let d = &rules::find(&page(t))[0];
    assert_eq!(&t[d.span.start..d.span.end], "7700012345678");
}

// ---------------------------------------------------- Separators and widths

#[test]
fn ssn_with_other_separators() {
    // Dots, en dash, U+2212 minus (PDFDocEncoding), a line break after a
    // dash, fullwidth digits. Spans point at the original text.
    for t in [
        "SSN 900.12.3456",
        "SSN 900\u{2013}12\u{2013}3456",
        "SSN 900\u{2212}12\u{2212}3456",
        "SSN 900\u{2010}12\u{2010}3456",
        "SSN 900-12-\n3456",
        "SSN \u{ff19}\u{ff10}\u{ff10}-\u{ff11}\u{ff12}-\u{ff13}\u{ff14}\u{ff15}\u{ff16}",
    ] {
        let want = &t[4..];
        assert!(has(t, TokenKind::Ssn, want), "{t:?}: {:?}", found(t));
    }
    // Mixed separators are not an SSN shape.
    assert!(none_of("ref 900.12-3456", TokenKind::Ssn));
}

#[test]
fn routing_with_spaces_after_a_label() {
    assert!(has(
        "ABA 011 200 446", // pii-check: allow (synthetic, constructed for the checksum test)
        TokenKind::Routing,
        "011 200 446" // pii-check: allow (synthetic, constructed for the checksum test)
    ));
    assert!(none_of("Total 011 200 446", TokenKind::Routing)); // pii-check: allow (synthetic, constructed for the checksum test)
}

#[test]
fn bare_ten_digit_phones() {
    assert!(has(
        "RIDESHARE TRIP 6145550142 OH",
        TokenKind::Phone,
        "6145550142"
    ));
    // Area or exchange starting 0 or 1: not a phone.
    assert!(none_of("ref 1145550142", TokenKind::Phone));
    assert!(none_of("ref 6141550142", TokenKind::Phone));
    // Glued to more digits: not a phone.
    assert!(none_of("ref 61455501421", TokenKind::Phone));
    // An account label wins.
    assert!(has("Account 6145550142", TokenKind::Account, "6145550142"));
}

#[test]
fn aba_rejects_all_zeros() {
    assert!(!rules::is_valid_aba("000000000"));
    assert!(none_of("Routing 000000000", TokenKind::Routing));
}

#[test]
fn mir_cards() {
    // Built from parts: a Luhn-valid number in the Mir range 2200-2204.
    let mir = format!("2200{}04", "0".repeat(10));
    assert_eq!(rules::card_brand(&mir), Some("mir"));
    let spaced = format!(
        "{} {} {} {}",
        &mir[..4],
        &mir[4..8],
        &mir[8..12],
        &mir[12..]
    );
    assert!(rules::is_valid_card(&spaced));
    assert_eq!(
        rules::card_brand(&format!("2205{}03", "0".repeat(10))),
        None
    );
}

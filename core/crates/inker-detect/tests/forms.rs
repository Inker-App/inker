//! Fixture-driven tests for every IRS form template in packs/tax.
//!
//! Each fixture under tests/fixtures/ is synthetic plain text in the shape of
//! a form's text layer. SSNs use area 9xx outside the ITIN groups, EINs use
//! unissued prefixes, and every other number is made up.

#![allow(clippy::unwrap_used, clippy::expect_used)] // tests may panic

use std::path::PathBuf;

use inker_core::{DetectionSource, Extracted, PageText, TextOrigin, TokenKind, Treatment};
use inker_detect::template;
use inker_packs::PackSet;

fn packs() -> PackSet {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../packs/tax");
    PackSet::load(Some(&dir)).unwrap()
}

pub fn fixture(name: &str) -> String {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    std::fs::read_to_string(p).unwrap()
}

#[derive(Debug, Clone, PartialEq)]
struct Want {
    field: String,
    kind: TokenKind,
    treatment: Treatment,
    role: Option<String>,
    value: String,
    generalized: Option<String>,
}

/// Tokenized value.
fn t(field: &str, kind: TokenKind, role: Option<&str>, value: &str) -> Want {
    Want {
        field: field.into(),
        kind,
        treatment: Treatment::Tokenize,
        role: role.map(String::from),
        value: value.into(),
        generalized: None,
    }
}

/// Generalized address.
fn g(field: &str, role: Option<&str>, value: &str, general: &str) -> Want {
    Want {
        field: field.into(),
        kind: TokenKind::Address,
        treatment: Treatment::Generalize,
        role: role.map(String::from),
        value: value.into(),
        generalized: Some(general.into()),
    }
}

fn custom(s: &str) -> TokenKind {
    TokenKind::Custom(s.into())
}

use TokenKind::{Account, Address, Ein, Org, Person, Phone, Ssn};

/// Match and apply the pack against a fixture and compare with `want`.
/// Every amount listed must be inside a keep span and outside every detection.
fn check(
    fixture_name: &str,
    form: &str,
    kind: &str,
    year: u16,
    mut want: Vec<Want>,
    amounts: &[&str],
) {
    let text = fixture(fixture_name);
    let e = Extracted {
        pages: vec![PageText {
            page: 1,
            text: text.clone(),
            origin: TextOrigin::TextLayer,
        }],
        fields: vec![],
    };
    let set = packs();
    let m =
        template::match_template(&e, &set).unwrap_or_else(|| panic!("{form}: no template matched"));
    assert_eq!(m.template.id, form);
    assert_eq!(m.template.kind, kind);
    assert_eq!(m.year, Some(year), "{form}: year");
    let out = template::apply(&e, &m);
    let mut got: Vec<Want> = out
        .detections
        .iter()
        .map(|d| {
            let DetectionSource::Template { form: f, field } = &d.source else {
                panic!("{form}: non-template source {:?}", d.source)
            };
            assert_eq!(f, form);
            assert!(
                d.confidence >= 0.9,
                "{form}/{field}: confidence {}",
                d.confidence
            );
            Want {
                field: field.clone(),
                kind: d.kind.clone(),
                treatment: d.treatment,
                role: d.role.clone(),
                value: text[d.span.start..d.span.end].to_string(),
                generalized: d.generalized.clone(),
            }
        })
        .collect();
    got.sort_by(|a, b| a.field.cmp(&b.field));
    want.sort_by(|a, b| a.field.cmp(&b.field));
    for w in &want {
        assert!(got.contains(w), "{form}: missing {w:#?}\n got: {got:#?}");
    }
    for g in &got {
        assert!(want.contains(g), "{form}: unexpected {g:#?}");
    }
    for a in amounts {
        let at = text
            .find(a)
            .unwrap_or_else(|| panic!("{form}: amount {a} not in fixture"));
        let end = at + a.len();
        // Keep spans cover only amount- and code-shaped tokens; words in a
        // keep box (a locality, a security description) stay visible to
        // fail-closed, and are not detected by the template either.
        let mut off = at;
        for tok in a.split(' ') {
            let (ts, te) = (off, off + tok.len());
            off = te + 1;
            let kept = out.keep.iter().any(|(_, s)| s.start <= ts && te <= s.end);
            assert_eq!(
                kept,
                inker_detect::shape::is_benign_token(tok),
                "{form}: keep of {tok:?} in {a:?}"
            );
        }
        assert!(
            !out.detections
                .iter()
                .any(|d| d.span.start < end && at < d.span.end),
            "{form}: amount {a} touched"
        );
    }
}

#[test]
fn w2() {
    check(
        "w2.txt",
        "w2",
        "w2",
        2025,
        vec![
            t("a", Ssn, Some("employee"), "900-14-2231"),
            t("b", Ein, Some("employer"), "07-4400123"),
            t("c_name", Org, Some("employer"), "Contoso Widgets LLC"),
            g(
                "c_address",
                Some("employer"),
                "500 Harbor Blvd Suite 200\nSpringfield, IL 62701",
                "Springfield, IL",
            ),
            t(
                "d",
                custom("control_number"),
                Some("employer"),
                "CW-0042-77",
            ),
            t("e", Person, Some("employee"), "Jane Q Doe"),
            g(
                "f",
                Some("employee"),
                "1428 Elm Street Apt 4B\nBrooklyn, NY 11201",
                "Brooklyn, NY",
            ),
            t("15_id", custom("state_id"), Some("employer"), "0047-91234"),
        ],
        &[
            "52340.18",
            "6120.44",
            "54000.00",
            "3348.00",
            "783.00",
            "1659.82",
            "412.10",
            "2590.83",
            "SPRINGFIELD",
        ],
    );
}

/// The payer/recipient block shared by the 1099 family.
struct Block<'a> {
    payer: &'a str,
    payer_addr: &'a str,
    payer_city: &'a str,
    payer_tin: &'a str,
    recipient_tin: &'a str,
    recipient: &'a str,
    street: &'a str,
    city_line: &'a str,
    city: &'a str,
    account: &'a str,
}

fn payer_recipient(b: Block<'_>) -> Vec<Want> {
    vec![
        t("payer_name", Org, Some("payer"), b.payer),
        g("payer_address", Some("payer"), b.payer_addr, b.payer_city),
        t("payer_tin", Ein, Some("payer"), b.payer_tin),
        t("recipient_tin", Ssn, Some("recipient"), b.recipient_tin),
        t("recipient_name", Person, Some("recipient"), b.recipient),
        t("recipient_street", Address, Some("recipient"), b.street),
        g("recipient_city", Some("recipient"), b.city_line, b.city),
        t("account", Account, Some("recipient"), b.account),
    ]
}

#[test]
fn f1099_int() {
    let mut want = payer_recipient(Block {
        payer: "Northwind Credit Union",
        payer_addr: "77 Lakeview Ave\nMadison, WI 53703",
        payer_city: "Madison, WI",
        payer_tin: "17-2233445",
        recipient_tin: "912-34-5678",
        recipient: "John A Sample",
        street: "22 Birch Rd Unit 5",
        city_line: "Madison, WI 53711",
        city: "Madison, WI",
        account: "5501-2299-87",
    });
    want.push(t(
        "payer_rtn",
        TokenKind::Routing,
        Some("payer"),
        "990000123",
    ));
    want.push(t("16", custom("state_id"), Some("payer"), "WI-0331-22"));
    check(
        "1099-int.txt",
        "1099-int",
        "1099-int",
        2025,
        want,
        &["1,204.55", "88.10", "31.20"],
    );
}

#[test]
fn f1099_nec() {
    let mut want = payer_recipient(Block {
        payer: "Fabrikam Consulting Inc",
        payer_addr: "900 Market St Fl 3\nDenver, CO 80202",
        payer_city: "Denver, CO",
        payer_tin: "28-1100220",
        recipient_tin: "945-67-8901",
        recipient: "Pat R Example",
        street: "77 Oak Ave",
        city_line: "Boise, ID 83702",
        city: "Boise, ID",
        account: "NEC-000417",
    });
    want.push(t("6", custom("state_id"), Some("payer"), "4471-0099"));
    check(
        "1099-nec.txt",
        "1099-nec",
        "1099-nec",
        2025,
        want,
        &["18,250.00", "612.00"],
    );
}

#[test]
fn f1099_div() {
    let mut want = payer_recipient(Block {
        payer: "Tailspin Brokerage LLC",
        payer_addr: "1 Commerce Plaza\nBoston, MA 02110",
        payer_city: "Boston, MA",
        payer_tin: "49-5500660",
        recipient_tin: "966-48-1122",
        recipient: "Chris Placeholder",
        street: "88 Pine St",
        city_line: "Portland, OR 97205",
        city: "Portland, OR",
        account: "TB-8812-4400",
    });
    want.push(t("15", custom("state_id"), Some("payer"), "MA-77120"));
    check(
        "1099-div.txt",
        "1099-div",
        "1099-div",
        2025,
        want,
        &["2,410.33", "2,001.10", "512.00", "31.45"],
    );
}

#[test]
fn f1099_b() {
    let want = payer_recipient(Block {
        payer: "Tailspin Brokerage LLC",
        payer_addr: "1 Commerce Plaza\nBoston, MA 02110",
        payer_city: "Boston, MA",
        payer_tin: "49-5500660",
        recipient_tin: "900-22-3344",
        recipient: "Alex Mock",
        street: "14 Cedar Ln",
        city_line: "Austin, TX 78701",
        city: "Austin, TX",
        account: "TB-5520-0091",
    });
    check(
        "1099-b.txt",
        "1099-b",
        "1099-b",
        2025,
        want,
        &[
            "14,820.00",
            "9,100.00",
            "03/14/2023",
            "11/02/2025",
            "100 sh. SAMPLE FUND",
            "SAMPLE001",
        ],
    );
}

#[test]
fn f1099_k() {
    let want = vec![
        t("filer_name", Org, Some("payer"), "Litware Payments Inc"),
        g(
            "filer_address",
            Some("payer"),
            "200 Mission St\nSan Francisco, CA 94105",
            "San Francisco, CA",
        ),
        t("filer_tin", Ein, Some("payer"), "69-1234500"),
        t("payee_tin", Ssn, Some("recipient"), "900-31-7788"),
        t(
            "payee_name",
            Person,
            Some("recipient"),
            "Maria Example-Lopez",
        ),
        t("payee_street", Address, Some("recipient"), "61 Willow Ct"),
        g(
            "payee_city",
            Some("recipient"),
            "Tucson, AZ 85701",
            "Tucson, AZ",
        ),
        t("pse_name", Org, Some("payer"), "Litware Payments Inc"),
        t("account", Account, Some("recipient"), "LP-0099-3311"),
        t("7", custom("state_id"), Some("payer"), "AZ-55012"),
    ];
    check(
        "1099-k.txt",
        "1099-k",
        "1099-k",
        2025,
        want,
        &[
            "24,118.90",
            "5999",
            "412",
            "2,001.00",
            "1,850.25",
            "2,400.00",
        ],
    );
}

#[test]
fn f1099_misc() {
    let mut want = payer_recipient(Block {
        payer: "Adatum Property Management LLC",
        payer_addr: "40 River Rd\nAlbany, NY 12207",
        payer_city: "Albany, NY",
        payer_tin: "78-0001234",
        recipient_tin: "900-41-9900",
        recipient: "Sam T Sample",
        street: "7 Hill St",
        city_line: "Troy, NY 12180",
        city: "Troy, NY",
        account: "AD-4410-22",
    });
    want.push(t("17", custom("state_id"), Some("payer"), "7700-1234"));
    check(
        "1099-misc.txt",
        "1099-misc",
        "1099-misc",
        2025,
        want,
        &["12,000.00", "450.00", "380.00"],
    );
}

#[test]
fn f1098() {
    let want = vec![
        t("lender_name", Org, Some("lender"), "Adatum Mortgage Co."),
        g(
            "lender_address",
            Some("lender"),
            "12 Bank St\nHartford, CT 06103",
            "Hartford, CT",
        ),
        t("lender_tin", Ein, Some("lender"), "89-4567123"),
        t("borrower_tin", Ssn, Some("borrower"), "912-08-4455"),
        t("borrower_name", Person, Some("borrower"), "Jane Q Doe"),
        t(
            "borrower_street",
            Address,
            Some("borrower"),
            "1428 Elm Street",
        ),
        g(
            "borrower_city",
            Some("borrower"),
            "Brooklyn, NY 11201",
            "Brooklyn, NY",
        ),
        t("account", Account, Some("borrower"), "ML-0012-9934"),
        g(
            "8",
            Some("borrower"),
            "1428 Elm Street, Brooklyn, NY 11201",
            "Brooklyn, NY",
        ),
    ];
    check(
        "1098.txt",
        "1098",
        "1098",
        2025,
        want,
        &["9,812.44", "402,110.00", "06/01/2021", "6,100.00"],
    );
}

#[test]
fn f1095_a() {
    let r = Some("recipient");
    let want = vec![
        t("2", Account, r, "POL-7712-0045"),
        t("3", Org, None, "Proseware Health Plan Inc"),
        t("4", Person, r, "Jane Q Doe"),
        t("5", Ssn, r, "900-14-2231"),
        t("6", custom("dob"), r, "04/12/1988"),
        t("7", Person, r, "John A Sample"),
        t("8", Ssn, r, "912-34-5678"),
        t("9", custom("dob"), r, "09/30/1987"),
        t("12", Address, r, "1428 Elm Street Apt 4B"),
        t("15", Address, r, "US 11201"),
        t("16_name", Person, r, "Jane Q Doe"),
        t("16_ssn", Ssn, r, "900-14-2231"),
        t("16_dob", custom("dob"), r, "04/12/1988"),
        t("17_name", Person, r, "John A Sample"),
        t("17_ssn", Ssn, r, "912-34-5678"),
        t("17_dob", custom("dob"), r, "09/30/1987"),
    ];
    check(
        "1095-a.txt",
        "1095-a",
        "1095-a",
        2025,
        want,
        &["01/01/2025", "12/31/2025", "Brooklyn"],
    );
}

#[test]
fn f1095_b() {
    let r = Some("recipient");
    let e = Some("employer");
    let want = vec![
        t("1", Person, r, "Pat R Example"),
        t("2", Ssn, r, "945-67-8901"),
        t("4", Address, r, "77 Oak Ave"),
        t("7", Address, r, "US 83702"),
        t("10", Org, e, "Fabrikam Consulting Inc"),
        t("11", Ein, e, "28-1100220"),
        t("12", Address, e, "900 Market St Fl 3"),
        t("15", Address, e, "US 80202"),
        t("16", Org, None, "Proseware Health Plan Inc"),
        t("17", Ein, None, "07-4400123"),
        t("18", Phone, None, "(800) 555-0123"),
        t("19", Address, None, "1 Health Way"),
        t("22", Address, None, "US 06103"),
        t("23_name", Person, r, "Pat R Example"),
        t("23_ssn", Ssn, r, "945-67-8901"),
        t("24_name", Person, r, "Robin Example"),
        t("24_dob", custom("dob"), r, "05/05/2015"),
    ];
    check(
        "1095-b.txt",
        "1095-b",
        "1095-b",
        2025,
        want,
        &["Boise", "Denver", "Hartford"],
    );
}

#[test]
fn f1095_c() {
    let r = Some("employee");
    let e = Some("employer");
    let want = vec![
        t("1", Person, r, "Jane Q Doe"),
        t("2", Ssn, r, "900-14-2231"),
        t("3", Address, r, "1428 Elm Street Apt 4B"),
        t("6", Address, r, "US 11201"),
        t("7", Org, e, "Contoso Widgets LLC"),
        t("8", Ein, e, "07-4400123"),
        t("9", Address, e, "500 Harbor Blvd Suite 200"),
        t("10", Phone, e, "(217) 555-0188"),
        t("13", Address, e, "US 62701"),
        t("17", Address, r, "11201"),
        t("18_name", Person, r, "Jane Q Doe"),
        t("18_ssn", Ssn, r, "900-14-2231"),
        t("19_name", Person, r, "Robin Doe"),
        t("19_dob", custom("dob"), r, "05/05/2015"),
    ];
    check(
        "1095-c.txt",
        "1095-c",
        "1095-c",
        2025,
        want,
        &["1E", "102.50", "2C", "Brooklyn", "Springfield"],
    );
}

#[test]
fn k1_1065() {
    let want = vec![
        t("A", Ein, Some("payer"), "18-7766554"),
        t("B_name", Org, Some("payer"), "Coho Partners LP"),
        g(
            "B_address",
            Some("payer"),
            "45 Vineyard Rd\nNapa, CA 94558",
            "Napa, CA",
        ),
        t("E", Ssn, Some("recipient"), "900-12-6655"),
        t("F_name", Person, Some("recipient"), "Alex Mock"),
        g(
            "F_address",
            Some("recipient"),
            "14 Cedar Ln\nAustin, TX 78701",
            "Austin, TX",
        ),
    ];
    check(
        "k1-1065.txt",
        "k1-1065",
        "k1-1065",
        2025,
        want,
        &[
            "8,420.00",
            "-1,200.00",
            "6,000.00",
            "45.12",
            "A 5,000.00",
            "Profit 2.5000%",
            "Nonrecourse 12,000.00",
        ],
    );
}

#[test]
fn k1_1120s() {
    let want = vec![
        t("A", Ein, Some("payer"), "19-3344556"),
        t("B_name", Org, Some("payer"), "Wingtip Ventures Inc"),
        g(
            "B_address",
            Some("payer"),
            "3 Summit Dr\nReno, NV 89501",
            "Reno, NV",
        ),
        t("E", Ssn, Some("recipient"), "966-48-1122"),
        t("F1_name", Person, Some("recipient"), "Chris Placeholder"),
        g(
            "F1_address",
            Some("recipient"),
            "88 Pine St\nPortland, OR 97205",
            "Portland, OR",
        ),
    ];
    check(
        "k1-1120s.txt",
        "k1-1120s",
        "k1-1120s",
        2025,
        want,
        &[
            "22,500.00",
            "120.00",
            "D 10,000.00",
            "25.000000%",
            "Beginning of tax year 250",
        ],
    );
}

#[test]
fn f1040() {
    // A joint return laid out as the IRS form: rows of column heads with the
    // typed values under them, two dependents, direct deposit, a designee
    // and a paid preparer.
    let want = vec![
        t(
            "name",
            Person,
            Some("taxpayer"),
            "Pat R                                 Example",
        ),
        t("ssn", Ssn, Some("taxpayer"), "900-33-1100"),
        t(
            "spouse_name",
            Person,
            Some("spouse"),
            "Sam T                                                     Sample",
        ),
        t("spouse_ssn", Ssn, Some("spouse"), "900-41-9900"),
        t(
            "street",
            Address,
            Some("taxpayer"),
            "1428 Elm Street Apt 4B",
        ),
        // A city alone cannot be generalized to "City, ST", so it is inked.
        t("city", Address, Some("taxpayer"), "Brooklyn"),
        t("zip", Address, Some("taxpayer"), "11201"),
        t(
            "dependent_1",
            Person,
            Some("dependent"),
            "Alex            Example",
        ),
        t("dependent_1_ssn", Ssn, Some("dependent"), "900-33-1200"),
        t(
            "dependent_2",
            Person,
            Some("dependent"),
            "Jo              Example",
        ),
        t("dependent_2_ssn", Ssn, Some("dependent"), "900-33-1300"),
        t("routing", TokenKind::Routing, Some("taxpayer"), "987654321"),
        t("account", Account, Some("taxpayer"), "4471009912"),
        t("designee_name", Person, Some("designee"), "Robin K Sample"),
        t("phone", Phone, None, "(212) 555-0101"),
        t("designee_pin", custom("pin"), Some("designee"), "12345"),
        t(
            "occupation",
            custom("occupation"),
            Some("taxpayer"),
            "Teacher",
        ),
        t("ip_pin", custom("ip_pin"), Some("taxpayer"), "000111"),
        t(
            "spouse_occupation",
            custom("occupation"),
            Some("spouse"),
            "Nurse",
        ),
        t("spouse_ip_pin", custom("ip_pin"), Some("spouse"), "000222"),
        t("phone", Phone, None, "(212) 555-0199"),
        t(
            "email",
            TokenKind::Email,
            Some("taxpayer"),
            "pat.example@example.com",
        ),
        t(
            "preparer_name",
            Person,
            Some("preparer"),
            "Chris Placeholder",
        ),
        t("ptin", custom("ptin"), Some("preparer"), "P00001234"),
        t("firm_name", Org, Some("preparer"), "Contoso Tax Prep LLC"),
        t("phone", Phone, None, "(217) 555-0142"),
        g(
            "firm_address",
            Some("preparer"),
            "500 Harbor Blvd, Springfield, IL 62701",
            "Springfield, IL",
        ),
        t("firm_ein", Ein, Some("preparer"), "07-4400555"),
    ];
    check(
        "1040.txt",
        "1040",
        "1040",
        2023,
        want,
        &[
            "84,250.00",
            "412.88",
            "1,004.10",
            "1,284.16",
            "18,500.00",
            "104,447.04",
            "1,307.00",
            "103,140.04",
            "27,700.00",
            "75,440.04",
            "8,628.00",
            "2,000.00",
            "6,628.00",
            "2,614.00",
            "9,242.00",
            "11,912.40",
            "2,670.40",
        ],
    );
}

#[test]
fn templates_do_not_cross_match() {
    // Each fixture matches its own template and no other.
    let set = packs();
    for (file, id) in [
        ("w2.txt", "w2"),
        ("1099-int.txt", "1099-int"),
        ("1099-nec.txt", "1099-nec"),
        ("1099-div.txt", "1099-div"),
        ("1099-b.txt", "1099-b"),
        ("1099-k.txt", "1099-k"),
        ("1099-misc.txt", "1099-misc"),
        ("1098.txt", "1098"),
        ("1095-a.txt", "1095-a"),
        ("1095-b.txt", "1095-b"),
        ("1095-c.txt", "1095-c"),
        ("k1-1065.txt", "k1-1065"),
        ("k1-1120s.txt", "k1-1120s"),
        ("1040.txt", "1040"),
    ] {
        let page = PageText {
            page: 1,
            text: fixture(file),
            origin: TextOrigin::TextLayer,
        };
        let matching: Vec<&str> = set
            .forms()
            .filter(|f| f.anchors.iter().all(|a| a.is_match(&page.text)))
            .map(|f| f.id.as_str())
            .collect();
        assert_eq!(matching, vec![id], "{file}");
    }
}

/// The text each template detection of `field` reads on `text`.
fn read_field(text: &str, field: &str) -> Vec<String> {
    let e = Extracted {
        pages: vec![PageText {
            page: 1,
            text: text.to_string(),
            origin: TextOrigin::TextLayer,
        }],
        fields: vec![],
    };
    let set = packs();
    let m = template::match_template(&e, &set).expect("a template");
    template::apply(&e, &m)
        .detections
        .iter()
        .filter(|d| matches!(&d.source, DetectionSource::Template { field: f, .. } if f == field))
        .map(|d| {
            assert_eq!(d.kind, Person, "{field} is a person");
            text[d.span.start..d.span.end].to_string()
        })
        .collect()
}

/// `fixture_name` with its first `from` replaced by `to`.
fn with_line(fixture_name: &str, from: &str, to: &str) -> String {
    let text = fixture(fixture_name);
    assert!(text.contains(from), "{fixture_name} has no {from:?}");
    text.replacen(from, to, 1)
}

#[test]
fn w2_box_e_reads_the_whole_name_across_its_columns() {
    // Box e as the IRS layout prints it: the label line with its three
    // column heads, then first name and initial, last name, suffix as
    // columns. The whole line is one Person: reading only the first column
    // left the surname in the clear (first end-to-end run, 2026-10-28).
    let sample = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../samples/synthetic/w2-acme-2025.txt"),
    )
    .unwrap();
    assert!(sample.contains("  Jordan A    Example\n"));
    assert_eq!(read_field(&sample, "e"), ["Jordan A    Example"]);

    let suffixed = with_line("w2.txt", "Jane Q Doe", "  Jane Q    Doe    Jr");
    assert_eq!(read_field(&suffixed, "e"), ["Jane Q    Doe    Jr"]);
}

#[test]
fn name_lines_on_every_form_read_the_whole_name() {
    let cases: &[(&str, &str, &str, &str, &str)] = &[
        // (fixture, line, split line, field, whole name read)
        (
            "1095-a.txt",
            "Jane Q Doe",
            "Jane Q    Doe",
            "4",
            "Jane Q    Doe",
        ),
        (
            "1095-b.txt",
            "Pat R Example\n",
            "Pat R    Example\n",
            "1",
            "Pat R    Example",
        ),
        (
            "1095-b.txt",
            "23 Pat R Example   945-67-8901   X",
            "23 Pat   R   Example   945-67-8901   X",
            "23_name",
            "Pat   R   Example",
        ),
        (
            "1095-b.txt",
            "24 Robin Example   05/05/2015   X",
            "24 Robin   Example   05/05/2015   X",
            "24_name",
            "Robin   Example",
        ),
        (
            "1095-c.txt",
            "Jane Q Doe\n",
            "Jane Q    Doe\n",
            "1",
            "Jane Q    Doe",
        ),
        (
            // A trailing checkbox mark is not an initial.
            "1095-c.txt",
            "19 Robin Doe   05/05/2015   X",
            "19 Robin   Doe   X",
            "19_name",
            "Robin   Doe",
        ),
        (
            "1098.txt",
            "Jane Q Doe",
            "Jane Q    Doe",
            "borrower_name",
            "Jane Q    Doe",
        ),
        (
            "1099-nec.txt",
            "Pat R Example",
            "Pat R    Example    Jr.",
            "recipient_name",
            "Pat R    Example    Jr.",
        ),
        (
            "1099-int.txt",
            "John A Sample",
            "John A    Sample",
            "recipient_name",
            "John A    Sample",
        ),
        (
            "k1-1065.txt",
            "Alex Mock",
            "Alex    Mock",
            "F_name",
            "Alex    Mock",
        ),
    ];
    for (fx, from, to, field, want) in cases {
        let text = with_line(fx, from, to);
        assert_eq!(read_field(&text, field), [*want], "{fx} field {field}");
    }
}

#[test]
fn a_name_line_stops_at_a_column_that_is_not_a_name_part() {
    // 1095-C line 1 printed with the SSN column on the same line.
    let text = with_line(
        "1095-c.txt",
        "Jane Q Doe\n",
        "Jane Q    Doe    900-14-2231    Employee\n",
    );
    assert_eq!(read_field(&text, "1"), ["Jane Q    Doe"]);
}

//! Behaviour of the template engine on a small demo template.

#![allow(clippy::unwrap_used, clippy::expect_used)] // tests may panic

use inker_core::{
    DetectionSource, Extracted, FormField, PageText, TextOrigin, TokenKind, Treatment,
};
use inker_detect::template;
use inker_packs::{Pack, PackSet};

const MANIFEST: &str = r#"
name = "test"
version = "0.0.1"
forms = ["demo", "demo-b"]
"#;

const DEMO: &str = r#"
id = "demo"
display = "Demo"
kind = "demo"
anchors = ["Demo Statement"]
year = 'Demo Statement\s+(20\d{2})'

[[fields]]
id = "tin"
label = "Recipient's TIN"
kind = "ssn"
treatment = "tokenize"
role = "recipient"

[[fields]]
id = "payer_tin"
label = "Payer's TIN"
kind = "ein"
treatment = "tokenize"
role = "payer"

[[fields]]
id = "name"
label = "Recipient's name"
kind = "person"
treatment = "tokenize"
role = "recipient"
acroform = ["f1_02"]

[[fields]]
id = "payer"
label = "Payer's name"
kind = "org"
treatment = "tokenize"
role = "payer"

[[fields]]
id = "addr"
label = "Recipient's name"
kind = "address"
treatment = "generalize"
role = "recipient"
skip_lines = 1
lines = 2

[[fields]]
id = "acct"
label = 'Account number \(see instructions\)'
kind = "account"
treatment = "tokenize"
role = "recipient"

[[fields]]
id = "dob"
label = "Date of birth"
kind = "dob"
treatment = "tokenize"
role = "recipient"
value = "date"

[[fields]]
id = "row16_name"
label = '(?m)^16[ \t]+'
kind = "person"
treatment = "tokenize"
role = "recipient"

[[fields]]
id = "row16_dob"
label = '(?m)^16[ \t]+'
kind = "dob"
treatment = "tokenize"
role = "recipient"
value = "date"

[[fields]]
id = "1"
label = '1\s+Interest income'
kind = "keep"
treatment = "keep"
"#;

fn demo_b() -> String {
    DEMO.replace("id = \"demo\"", "id = \"demo-b\"")
        .replace("kind = \"demo\"", "kind = \"demo-b\"")
        .replace(
            "anchors = [\"Demo Statement\"]",
            "anchors = [\"Demo Statement\", \"Variant B\"]",
        )
}

fn packs() -> PackSet {
    let b = demo_b();
    PackSet {
        packs: vec![Pack::from_sources(MANIFEST, &[("demo", DEMO), ("demo-b", &b)]).unwrap()],
    }
}

fn doc(text: &str, origin: TextOrigin) -> Extracted {
    Extracted {
        pages: vec![PageText {
            page: 1,
            text: text.into(),
            origin,
        }],
        fields: vec![],
    }
}

const PAGE: &str = "\
Demo Statement 2025
Payer's name
Northwind Trading Co.
Payer's TIN: 07-4400123
Recipient's TIN   900-14-2231   1 Interest income   1,234.56
Recipient's name
Jane Q Doe
1428 Elm Street
Brooklyn, NY 11201
Account number (see instructions)
4471-0099-12
";

fn values(
    text: &str,
    out: &template::TemplateOutput,
) -> Vec<(String, TokenKind, Treatment, Option<String>, String)> {
    out.detections
        .iter()
        .map(|d| {
            let field = match &d.source {
                DetectionSource::Template { form, field } => {
                    assert_eq!(form, "demo");
                    field.clone()
                }
                s => panic!("not a template source: {s:?}"),
            };
            (
                field,
                d.kind.clone(),
                d.treatment,
                d.role.clone(),
                text[d.span.start..d.span.end].to_string(),
            )
        })
        .collect()
}

#[test]
fn anchors_pick_the_template() {
    let p = packs();
    let m = template::match_template(&doc(PAGE, TextOrigin::TextLayer), &p).unwrap();
    assert_eq!(m.template.id, "demo");
    assert_eq!(m.pages, vec![1]);
    assert_eq!(m.year, Some(2025));
    // More anchors matched wins.
    let m = template::match_template(
        &doc(&format!("{PAGE}\nVariant B"), TextOrigin::TextLayer),
        &p,
    )
    .unwrap();
    assert_eq!(m.template.id, "demo-b");
    // Anchors are required.
    assert!(
        template::match_template(
            &doc("Recipient's TIN 900-14-2231", TextOrigin::TextLayer),
            &p
        )
        .is_none()
    );
}

#[test]
fn reads_each_field_shape() {
    let p = packs();
    let e = doc(PAGE, TextOrigin::TextLayer);
    let m = template::match_template(&e, &p).unwrap();
    let out = template::apply(&e, &m);
    let mut got = values(PAGE, &out);
    got.sort_by(|a, b| a.0.cmp(&b.0));
    let s = |x: &str| x.to_string();
    let r = |x: &str| Some(x.to_string());
    let mut want = vec![
        (
            s("payer"),
            TokenKind::Org,
            Treatment::Tokenize,
            r("payer"),
            s("Northwind Trading Co."),
        ),
        (
            s("payer_tin"),
            TokenKind::Ein,
            Treatment::Tokenize,
            r("payer"),
            s("07-4400123"),
        ),
        (
            s("tin"),
            TokenKind::Ssn,
            Treatment::Tokenize,
            r("recipient"),
            s("900-14-2231"),
        ),
        (
            s("name"),
            TokenKind::Person,
            Treatment::Tokenize,
            r("recipient"),
            s("Jane Q Doe"),
        ),
        (
            s("addr"),
            TokenKind::Address,
            Treatment::Generalize,
            r("recipient"),
            s("1428 Elm Street\nBrooklyn, NY 11201"),
        ),
        (
            s("acct"),
            TokenKind::Account,
            Treatment::Tokenize,
            r("recipient"),
            s("4471-0099-12"),
        ),
    ];
    want.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(got, want);
    let addr = out
        .detections
        .iter()
        .find(|d| d.kind == TokenKind::Address)
        .unwrap();
    assert_eq!(addr.generalized.as_deref(), Some("Brooklyn, NY"));
    for d in &out.detections {
        assert!(
            d.confidence >= 0.9,
            "template detections are high confidence"
        );
        assert!(!d.needs_review);
        assert_eq!(d.page, 1);
    }
    // The keep field's value is recorded, not detected.
    let kept: Vec<&str> = out
        .keep
        .iter()
        .map(|(_, s)| &PAGE[s.start..s.end])
        .collect();
    assert_eq!(kept, vec!["1,234.56"]);
    // Role regions cover the values with roles.
    assert!(
        out.regions
            .iter()
            .any(|r| r.role == "payer" && PAGE[r.span.start..r.span.end].contains("07-4400123"))
    );
}

#[test]
fn missing_values_are_not_invented() {
    let text = "Demo Statement 2025\nRecipient's name\nRecipient's TIN\nPayer's name\n\n";
    let p = packs();
    let e = doc(text, TextOrigin::TextLayer);
    let m = template::match_template(&e, &p).unwrap();
    let out = template::apply(&e, &m);
    assert!(out.detections.is_empty(), "{:?}", values(text, &out));
}

#[test]
fn unparseable_address_is_tokenized_not_generalized() {
    let text = "Demo Statement 2025\nRecipient's name\nJane Q Doe\nSomewhere Over\nThe Rainbow\n";
    let p = packs();
    let e = doc(text, TextOrigin::TextLayer);
    let m = template::match_template(&e, &p).unwrap();
    let out = template::apply(&e, &m);
    let addr = out
        .detections
        .iter()
        .find(|d| d.kind == TokenKind::Address)
        .unwrap();
    assert_eq!(addr.treatment, Treatment::Tokenize);
    assert_eq!(addr.generalized, None);
}

#[test]
fn every_copy_on_a_page_is_read() {
    let text = format!("{PAGE}\n--- Copy C ---\nRecipient's TIN: 900-14-2231\n");
    let p = packs();
    let e = doc(&text, TextOrigin::TextLayer);
    let m = template::match_template(&e, &p).unwrap();
    let out = template::apply(&e, &m);
    assert_eq!(
        out.detections
            .iter()
            .filter(|d| d.kind == TokenKind::Ssn)
            .count(),
        2
    );
}

#[test]
fn ocr_pages_lower_confidence() {
    let p = packs();
    let e = doc(PAGE, TextOrigin::Ocr);
    let m = template::match_template(&e, &p).unwrap();
    let out = template::apply(&e, &m);
    assert!(!out.detections.is_empty());
    for d in &out.detections {
        assert!(
            d.confidence < 0.9 && d.confidence >= 0.75,
            "{}",
            d.confidence
        );
    }
}

#[test]
fn acroform_fields_are_read_by_name() {
    let p = packs();
    let mut e = doc("Demo Statement 2025\n", TextOrigin::TextLayer);
    e.fields = vec![
        FormField {
            name: "topmostSubform[0].CopyB[0].f1_02[0]".into(),
            value: "Jane Q Doe".into(),
            page: Some(1),
        },
        FormField {
            name: "topmostSubform[0].CopyB[0].f1_99[0]".into(),
            value: "12.00".into(),
            page: Some(1),
        },
        FormField {
            name: "f1_02".into(),
            value: "  ".into(),
            page: Some(1),
        },
    ];
    let m = template::match_template(&e, &p).unwrap();
    let out = template::apply(&e, &m);
    assert_eq!(out.detections.len(), 1);
    let d = &out.detections[0];
    assert_eq!(d.kind, TokenKind::Person);
    assert_eq!(
        d.field.as_deref(),
        Some("topmostSubform[0].CopyB[0].f1_02[0]")
    );
    assert_eq!((d.span.start, d.span.end), (0, "Jane Q Doe".len()));
    assert_eq!(d.role.as_deref(), Some("recipient"));
}

#[test]
fn dates_and_table_rows() {
    let text = "Demo Statement 2025\nDate of birth   04/12/1988\n16 Pat Example   900-14-2231   09/30/1987   01/01/2025\n17 \n";
    let p = packs();
    let e = doc(text, TextOrigin::TextLayer);
    let m = template::match_template(&e, &p).unwrap();
    let out = template::apply(&e, &m);
    let got: Vec<(String, String)> = values(text, &out).into_iter().map(|v| (v.0, v.4)).collect();
    let want: Vec<(String, String)> = [
        ("dob", "04/12/1988"),
        ("row16_name", "Pat Example"),
        ("row16_dob", "09/30/1987"),
    ]
    .iter()
    .map(|(a, b)| (a.to_string(), b.to_string()))
    .collect();
    for w in &want {
        assert!(got.contains(w), "missing {w:?} in {got:?}");
    }
    let dob = out
        .detections
        .iter()
        .find(|d| d.kind == TokenKind::Custom("dob".into()))
        .unwrap();
    assert_eq!(dob.treatment, Treatment::Tokenize);
}

#[test]
fn boilerplate_lines_are_never_values() {
    let b = demo_b();
    let p = PackSet {
        packs: vec![
            Pack::from_sources(MANIFEST, &[("demo", DEMO), ("demo-b", &b)])
                .unwrap()
                .with_boilerplate("Part II   Information About the Payer\n"),
        ],
    };
    let text = "Demo Statement 2025\nRecipient's name\nPart II   Information About the Payer\nPayer's name\n";
    let e = doc(text, TextOrigin::TextLayer);
    let m = template::match_template(&e, &p).unwrap();
    let out = template::apply(&e, &m);
    assert!(out.detections.is_empty(), "{:?}", values(text, &out));
}

/// A form laid out as rows of column heads with the typed values in the
/// columns under them (Form 1040's identity, signature and preparer rows).
const ROWS: &str = r#"
id = "rows"
display = "Rows"
kind = "rows"
anchors = ["Rows Return"]

[[fields]]
id = "name"
label = "First name and middle initial"
kind = "person"
treatment = "tokenize"
role = "taxpayer"
value = "below"

[[fields]]
id = "ssn"
label = "Social security number"
kind = "ssn"
treatment = "tokenize"
role = "taxpayer"
value = "below"

[[fields]]
id = "phone"
label = 'Phone no\.'
kind = "phone"
treatment = "tokenize"
value = "below"

[[fields]]
id = "occupation"
label = "Occupation"
kind = "occupation"
treatment = "tokenize"
role = "taxpayer"
value = "below"

[[fields]]
id = "sub"
label = "Sub-head"
kind = "keep"
treatment = "keep"

[[fields]]
id = "row_2"
label = 'Dependent \(1\) Name'
kind = "person"
treatment = "tokenize"
role = "dependent"
value = "below"
skip_lines = 1
"#;

fn rows_packs() -> PackSet {
    let manifest = "name = \"rows\"\nversion = \"0.0.1\"\nforms = [\"rows\"]\n";
    PackSet {
        packs: vec![Pack::from_sources(manifest, &[("rows", ROWS)]).unwrap()],
    }
}

/// `(field, value)` of every detection, sorted by field.
fn rows_values(text: &str) -> Vec<(String, String)> {
    let p = rows_packs();
    let e = doc(text, TextOrigin::TextLayer);
    let m = template::match_template(&e, &p).unwrap();
    let mut got: Vec<(String, String)> = template::apply(&e, &m)
        .detections
        .iter()
        .map(|d| match &d.source {
            DetectionSource::Template { field, .. } => {
                (field.clone(), text[d.span.start..d.span.end].to_string())
            }
            s => panic!("not a template source: {s:?}"),
        })
        .collect();
    got.sort();
    got
}

#[test]
fn below_reads_the_column_under_the_label() {
    // Each value sits under its own column head, whatever is typed (or not)
    // in the columns before it.
    let text = "\
Rows Return 2023
First name and middle initial   Last name   Social security number
  Jordan A                      Example     900-12-3456
Signature           Date          Occupation        Phone no.
                    04/12/2024    Field Engineer    (503) 555-0118
Dependent (1) Name   Age
  Casey Example      7
  Robin Example      4
";
    let s = |a: &str, b: &str| (a.to_string(), b.to_string());
    assert_eq!(
        rows_values(text),
        vec![
            s("name", "Jordan A                      Example"),
            s("occupation", "Field Engineer"),
            s("phone", "(503) 555-0118"),
            s("row_2", "Robin Example"),
            s("ssn", "900-12-3456"),
        ]
    );
}

#[test]
fn below_finds_nothing_in_an_empty_column() {
    // An empty cell is not filled from the next column, and a row of labels
    // under the label is never read as values.
    let text = "\
Rows Return 2023
Signature           Date          Occupation        Phone no.
                    04/12/2024                      (503) 555-0118
First name and middle initial   Social security number
Dependent (1) Name   Relationship
";
    assert_eq!(
        rows_values(text),
        vec![("phone".to_string(), "(503) 555-0118".to_string())]
    );
}

#[test]
fn below_looks_past_a_row_of_sub_heads() {
    // Form 1040's dependents table has a second row of column heads between
    // the first heads and the typed rows.
    let text = "\
Rows Return 2023
First name and middle initial   Social security number
                                Sub-head
  Jordan A                      900-12-3456
";
    let s = |a: &str, b: &str| (a.to_string(), b.to_string());
    assert_eq!(
        rows_values(text),
        vec![s("name", "Jordan A"), s("ssn", "900-12-3456")]
    );
}

#[test]
fn a_kept_amount_is_read_when_another_box_shares_its_row() {
    // Form 1040 prints two lines per row ("2a Tax-exempt interest  0.00
    // b Taxable interest  412.88"): the first amount is still the value of
    // its line, though the rest of the row holds another label.
    let text = "Demo Statement 2025\n1 Interest income   12.00   Recipient's TIN   900-14-2231\n";
    let p = packs();
    let e = doc(text, TextOrigin::TextLayer);
    let m = template::match_template(&e, &p).unwrap();
    let out = template::apply(&e, &m);
    let kept: Vec<&str> = out
        .keep
        .iter()
        .map(|(_, s)| &text[s.start..s.end])
        .collect();
    assert_eq!(kept, vec!["12.00"]);
}

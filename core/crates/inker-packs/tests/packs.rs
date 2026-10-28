#![allow(clippy::unwrap_used, clippy::expect_used)] // tests may panic

use std::path::PathBuf;

use inker_core::{DocKind, TokenKind, Treatment};
use inker_packs::{
    FieldKind, FormTemplate, Pack, PackError, PackSet, ValueShape, parse_field_kind,
};

const MANIFEST: &str = r#"
name = "test"
version = "0.0.1"
forms = ["demo"]
[codename]
default = "doc"
[codename.short]
demo = "dm"
"#;

const DEMO: &str = r#"
id = "demo"
display = "Demo"
kind = "demo"
anchors = ["Demo Statement"]
year = 'Demo Statement (20\d{2})'

[[fields]]
id = "a"
label = "a Recipient's TIN"
kind = "ssn"
treatment = "tokenize"
role = "recipient"

[[fields]]
id = "name"
label = "Recipient's name"
kind = "person"
treatment = "tokenize"
role = "recipient"
acroform = ["f1_02"]

[[fields]]
id = "addr"
label = "Recipient's name"
kind = "address"
treatment = "generalize"
role = "recipient"
skip_lines = 1
lines = 2

[[fields]]
id = "1"
label = '1\s+Interest income'
kind = "keep"
treatment = "keep"
"#;

fn tax_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../packs/tax")
}

#[test]
fn parses_a_template_with_defaults() {
    let t = FormTemplate::parse("demo.toml", DEMO).unwrap();
    assert_eq!(t.id, "demo");
    assert_eq!(t.fields.len(), 4);
    let a = &t.fields[0];
    assert_eq!(a.kind, FieldKind::Token(TokenKind::Ssn));
    assert_eq!(a.value, ValueShape::Tin, "ssn defaults to the tin shape");
    assert_eq!(a.role.as_deref(), Some("recipient"));
    assert!(
        a.label.is_match("A RECIPIENT'S TIN"),
        "labels are case-insensitive"
    );
    let name = &t.fields[1];
    assert_eq!(name.value, ValueShape::Line);
    assert_eq!(name.acroform, vec!["f1_02".to_string()]);
    let addr = &t.fields[2];
    assert_eq!(addr.value, ValueShape::Lines);
    assert_eq!((addr.skip_lines, addr.lines), (1, 2));
    assert_eq!(addr.treatment, Treatment::Generalize);
    assert_eq!(t.fields[3].kind, FieldKind::Keep);
    let y = t
        .year
        .as_ref()
        .unwrap()
        .captures("Demo Statement 2025")
        .unwrap();
    assert_eq!(&y[1], "2025");
}

#[test]
fn value_shapes_parse() {
    let t = FormTemplate::parse(
        "d.toml",
        &DEMO.replace(
            "role = \"recipient\"\nacroform",
            "role = \"recipient\"\nvalue = \"date\"\nacroform",
        ),
    )
    .unwrap();
    assert_eq!(t.fields[1].value, ValueShape::Date);
    assert!(matches!(
        FormTemplate::parse(
            "d.toml",
            &DEMO.replace("acroform = [", "value = \"nope\"\nacroform = [")
        ),
        Err(PackError::Parse { .. })
    ));
}

#[test]
fn field_kinds_parse() {
    let cases = [
        ("keep", FieldKind::Keep),
        ("ssn", FieldKind::Token(TokenKind::Ssn)),
        ("itin", FieldKind::Token(TokenKind::Itin)),
        ("ein", FieldKind::Token(TokenKind::Ein)),
        ("account", FieldKind::Token(TokenKind::Account)),
        ("routing", FieldKind::Token(TokenKind::Routing)),
        ("card", FieldKind::Token(TokenKind::Card)),
        ("person", FieldKind::Token(TokenKind::Person)),
        ("org", FieldKind::Token(TokenKind::Org)),
        ("address", FieldKind::Token(TokenKind::Address)),
        ("email", FieldKind::Token(TokenKind::Email)),
        ("phone", FieldKind::Token(TokenKind::Phone)),
        ("gov_id", FieldKind::Token(TokenKind::GovId)),
        (
            "state_id",
            FieldKind::Token(TokenKind::Custom("state_id".into())),
        ),
    ];
    for (s, want) in cases {
        assert_eq!(parse_field_kind(s), want, "{s}");
    }
}

fn invalid(text: &str) -> PackError {
    FormTemplate::parse("bad.toml", text).unwrap_err()
}

#[test]
fn rejects_bad_templates() {
    let base = DEMO.replace("[[fields]]", "[[zzz]]");
    // Not TOML.
    assert!(matches!(invalid("id = "), PackError::Parse { .. }));
    // Unknown key.
    assert!(matches!(
        invalid(&format!("{DEMO}\nbogus = 1\n")),
        PackError::Parse { .. }
    ));
    // Bad regex names the field.
    match invalid(&DEMO.replace("a Recipient's TIN", "a Recipient's (TIN")) {
        PackError::Regex { field, file, .. } => {
            assert_eq!(field, "a");
            assert_eq!(file, "bad.toml");
        }
        e => panic!("wrong error {e:?}"),
    }
    // Unknown role.
    assert!(matches!(
        invalid(&DEMO.replacen("role = \"recipient\"", "role = \"landlord\"", 1)),
        PackError::Invalid { .. }
    ));
    // Keep kind with a tokenize treatment.
    assert!(matches!(
        invalid(&DEMO.replace(
            "kind = \"keep\"\ntreatment = \"keep\"",
            "kind = \"keep\"\ntreatment = \"tokenize\""
        )),
        PackError::Invalid { .. }
    ));
    // Sensitive kind with a keep treatment.
    assert!(matches!(
        invalid(&DEMO.replacen("treatment = \"tokenize\"", "treatment = \"keep\"", 1)),
        PackError::Invalid { .. }
    ));
    // Duplicate field id.
    assert!(matches!(
        invalid(&DEMO.replace("id = \"name\"", "id = \"a\"")),
        PackError::Invalid { .. }
    ));
    // No anchors.
    assert!(matches!(
        invalid(&DEMO.replace("anchors = [\"Demo Statement\"]", "anchors = []")),
        PackError::Invalid { .. }
    ));
    // Year regex without a capture group.
    assert!(matches!(
        invalid(&DEMO.replace(
            r"year = 'Demo Statement (20\d{2})'",
            "year = 'Demo Statement'"
        )),
        PackError::Invalid { .. }
    ));
    // Fields must be [[fields]]: an unknown table is a schema error.
    assert!(matches!(invalid(&base), PackError::Parse { .. }));
}

#[test]
fn pack_from_sources_checks_the_form_list() {
    let p = Pack::from_sources(MANIFEST, &[("demo", DEMO)]).unwrap();
    assert_eq!(p.manifest.name, "test");
    assert_eq!(p.forms.len(), 1);
    // A listed form that is missing.
    assert!(matches!(
        Pack::from_sources(MANIFEST, &[]),
        Err(PackError::Invalid { .. })
    ));
    // Two templates with the same id.
    let m2 = MANIFEST.replace("forms = [\"demo\"]", "forms = [\"demo\", \"demo2\"]");
    assert!(Pack::from_sources(&m2, &[("demo", DEMO), ("demo2", DEMO)]).is_err());
    // Bad manifest.
    assert!(matches!(
        Pack::from_sources("name = 1", &[]),
        Err(PackError::Parse { .. })
    ));
}

#[test]
fn codename_short_falls_back_to_default() {
    let set = PackSet {
        packs: vec![Pack::from_sources(MANIFEST, &[("demo", DEMO)]).unwrap()],
    };
    assert_eq!(set.codename_short(&DocKind("demo".into())), Some("dm"));
    assert_eq!(set.codename_short(&DocKind("other".into())), Some("doc"));
}

const TAX_FORMS: &[(&str, &str)] = &[
    ("w2", "w2"),
    ("1099-int", "1099int"),
    ("1099-nec", "1099nec"),
    ("1099-div", "1099div"),
    ("1099-b", "1099b"),
    ("1099-k", "1099k"),
    ("1099-misc", "1099misc"),
    ("1098", "1098"),
    ("1095-a", "1095a"),
    ("1095-b", "1095b"),
    ("1095-c", "1095c"),
    ("k1-1065", "k1"),
    ("k1-1120s", "k1"),
];

#[test]
fn builtin_tax_pack_loads_every_form() {
    let set = PackSet::builtin().unwrap();
    assert_eq!(set.packs.len(), 1);
    assert_eq!(set.packs[0].manifest.name, "tax");
    let kinds: Vec<&str> = set.forms().map(|f| f.kind.as_str()).collect();
    for (kind, short) in TAX_FORMS {
        assert!(kinds.contains(kind), "missing template for {kind}");
        assert_eq!(
            set.codename_short(&DocKind((*kind).into())),
            Some(*short),
            "{kind}"
        );
    }
    for f in set.forms() {
        assert!(!f.fields.is_empty(), "{} has no fields", f.id);
        assert!(f.year.is_some(), "{} has no year regex", f.id);
    }
    assert_eq!(
        set.codename_short(&DocKind("bank-statement".into())),
        Some("bank")
    );
}

#[test]
fn override_dir_matches_builtin() {
    let disk = PackSet::load(Some(&tax_dir())).unwrap();
    let builtin = PackSet::load(None).unwrap();
    let a: Vec<&str> = disk.forms().map(|f| f.id.as_str()).collect();
    let b: Vec<&str> = builtin.forms().map(|f| f.id.as_str()).collect();
    assert_eq!(a, b);
    assert!(PackSet::load_dir(&tax_dir().join("nope")).is_err());
}

#[test]
fn errors_name_the_line_and_convert_to_core_errors() {
    let e = FormTemplate::parse("bad.toml", "id = \"x\"\nanchors = [\n").unwrap_err();
    assert!(e.to_string().starts_with("bad.toml: line "), "{e}");
    let core: inker_core::Error = e.into();
    assert!(matches!(core, inker_core::Error::Detect(_)));
}

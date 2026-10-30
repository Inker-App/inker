//! The IRS's own text layers: titles and labels wrap over lines, every label
//! comes first and the typed values after (as a flattened PDF reads), the
//! revision date sits next to the title, and a cover page talks about other
//! forms. Fixtures `irs-*.txt` are the IRS Copy B text with synthetic values
//! appended (SSN areas 9xx, unissued EIN prefixes, invented names); a form
//! feed separates pages.

#![allow(clippy::unwrap_used, clippy::expect_used)] // tests may panic

use std::path::PathBuf;

use inker_core::{Detection, DetectionSource, Extracted, PageText, TextOrigin, Treatment};
use inker_detect::{Classification, detect, template};
use inker_packs::PackSet;

fn packs() -> PackSet {
    PackSet::load(Some(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../packs/tax"),
    ))
    .unwrap()
}

fn fixture(name: &str) -> String {
    std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name),
    )
    .unwrap()
}

fn pages(text: &str) -> Extracted {
    Extracted {
        pages: text
            .split('\u{c}')
            .enumerate()
            .map(|(i, t)| PageText {
                page: i as u32 + 1,
                text: t.into(),
                origin: TextOrigin::TextLayer,
            })
            .collect(),
        fields: vec![],
    }
}

fn run(name: &str) -> (Extracted, Vec<Detection>, Classification) {
    let e = pages(&fixture(name));
    let (d, c) = detect(&e, &packs()).unwrap();
    (e, d, c)
}

fn value<'a>(e: &'a Extracted, d: &Detection) -> &'a str {
    let p = e.pages.iter().find(|p| p.page == d.page).unwrap();
    &p.text[d.span.start..d.span.end]
}

/// Every value in `want` is wholly inside some detection.
fn inked(e: &Extracted, dets: &[Detection], want: &[&str]) {
    for w in want {
        let (page, at) = e
            .pages
            .iter()
            .find_map(|p| p.text.rfind(w).map(|i| (p.page, i)))
            .unwrap_or_else(|| panic!("{w} not in fixture"));
        assert!(
            dets.iter().any(|d| d.page == page
                && d.field.is_none()
                && d.span.start <= at
                && at + w.len() <= d.span.end),
            "{w} is not inked"
        );
    }
}

/// Template detections only ever cover typed values, never label text.
fn template_values_only(e: &Extracted, dets: &[Detection], first_value: &str) {
    for d in dets {
        if !matches!(d.source, DetectionSource::Template { .. }) {
            continue;
        }
        let p = e.pages.iter().find(|p| p.page == d.page).unwrap();
        let start = p.text.rfind(first_value).unwrap();
        assert!(
            d.span.start >= start,
            "template read label text as a value: {:?} ({:?})",
            value(e, d),
            d.source
        );
    }
}

#[test]
fn misc_2025_wrapped_title_is_recognised() {
    let (e, dets, c) = run("irs-1099-misc-2025.txt");
    assert_eq!((c.kind.as_str(), c.year), ("1099-misc", Some(2025)));
    template_values_only(&e, &dets, "Adatum Property");
    inked(
        &e,
        &dets,
        &[
            "78-0001234",
            "900-41-9900",
            "Adatum Property Management LLC",
        ],
    );
}

#[test]
fn f1098_wrapped_title_is_recognised() {
    let (e, dets, c) = run("irs-1098-2025.txt");
    assert_eq!((c.kind.as_str(), c.year), ("1098", Some(2025)));
    template_values_only(&e, &dets, "Adatum Mortgage");
    inked(&e, &dets, &["17-2233445", "912-34-5678"]);
}

#[test]
fn misc_2026_is_not_nec_because_of_its_cover_page() {
    let (e, dets, c) = run("irs-1099-misc-2026.txt");
    assert_eq!((c.kind.as_str(), c.year), ("1099-misc", Some(2026)));
    template_values_only(&e, &dets, "Adatum Property");
    inked(
        &e,
        &dets,
        &[
            "78-0001234",
            "900-41-9900",
            "Adatum Property Management LLC",
        ],
    );
    let set = packs();
    let m = template::match_template(&e, &set).unwrap();
    assert_eq!(m.template.id, "1099-misc-2026");
    assert_eq!(m.pages, vec![2], "the cover page is not a form page");
}

#[test]
fn nec_and_int_are_recognised() {
    let (e, dets, c) = run("irs-1099-nec-2025.txt");
    assert_eq!((c.kind.as_str(), c.year), ("1099-nec", Some(2025)));
    template_values_only(&e, &dets, "Fabrikam Consulting");
    inked(&e, &dets, &["28-1100220", "945-67-8901"]);
    let (e, dets, c) = run("irs-1099-int-2025.txt");
    // "(Rev. January 2024)" is the revision, not the tax year.
    assert_eq!((c.kind.as_str(), c.year), ("1099-int", Some(2025)));
    template_values_only(&e, &dets, "Northwind Credit");
    inked(&e, &dets, &["17-2233445", "912-34-5678"]);
}

#[test]
fn year_skips_revision_dates() {
    let set = packs();
    // No "For calendar year": the year after the title, not the revision.
    let e = pages("Form 1099-INT (Rev. January 2024) Interest Income 2025\nPAYER'S TIN\n");
    assert_eq!(template::match_template(&e, &set).unwrap().year, Some(2025));
    let e = pages("Form 1099-INT\n(Rev. 1-2024)\nInterest \nIncome\nFor calendar year\n2026\n");
    assert_eq!(template::match_template(&e, &set).unwrap().year, Some(2026));
    let e = pages("Form 1099-INT\n(Rev. January 2024)\nInterest \nIncome\n");
    assert_eq!(template::match_template(&e, &set).unwrap().year, None);
}

#[test]
fn prose_mentions_do_not_match_a_template() {
    let set = packs();
    let e = pages(
        "Attention: we issued an April 2025 revision of Form 1099-NEC, \nNonemployee Compensation, to use first.\n",
    );
    assert!(template::match_template(&e, &set).is_none());
    let e = pages("File Form W-2c, Corrected Wage and Tax Statement, with the SSA.\n");
    assert!(template::match_template(&e, &set).is_none());
}

/// The per-box layout with the payer label wrapped as the IRS prints it.
fn wrapped(name: &str) -> String {
    let t = fixture(name);
    let w = t.replace("country, ZIP or foreign", "country, ZIP \nor foreign");
    assert_ne!(t, w);
    w
}

#[test]
fn wrapped_payer_label_does_not_become_the_payer() {
    let set = packs();
    for (file, org, addr) in [
        (
            "1099-nec.txt",
            "Fabrikam Consulting Inc",
            "900 Market St Fl 3\nDenver, CO 80202",
        ),
        ("1099-int.txt", "Northwind Credit Union", ""),
    ] {
        let text = wrapped(file);
        let e = pages(&text);
        let (dets, c) = detect(&e, &set).unwrap();
        assert_eq!(c.org.as_deref(), Some(org), "{file}");
        let name = dets
            .iter()
            .find(|d| matches!(&d.source, DetectionSource::Template { field, .. } if field == "payer_name"))
            .unwrap();
        assert_eq!(value(&e, name), org, "{file}");
        if !addr.is_empty() {
            let a = dets
                .iter()
                .find(|d| matches!(&d.source, DetectionSource::Template { field, .. } if field == "payer_address"))
                .unwrap();
            assert_eq!(value(&e, a), addr, "{file}");
        }
    }
}

#[test]
fn w2_copy_with_wrapped_box_13() {
    let (e, dets, c) = run("irs-w2-2025.txt");
    assert_eq!((c.kind.as_str(), c.year), ("w2", Some(2025)));
    template_values_only(&e, &dets, "900-14-2231");
    inked(
        &e,
        &dets,
        &["900-14-2231", "07-4400123", "Contoso Widgets LLC"],
    );
    // Box 13's wrapped label is one label, not a value.
    let set = packs();
    let m = template::match_template(&e, &set).unwrap();
    let out = template::apply(&e, &m);
    let p = &e.pages[0].text;
    let at = p.find("employee Retirement").unwrap();
    assert!(!out.keep.iter().any(|(_, s)| s.start <= at && at < s.end));
}

/// The page text an agent would see: tokenized and held spans become a
/// token, generalized spans their generalization, kept spans stay.
fn visible(e: &Extracted, dets: &[Detection]) -> String {
    let mut out = String::new();
    for p in &e.pages {
        let mut on: Vec<&Detection> = dets
            .iter()
            .filter(|d| d.page == p.page && d.field.is_none())
            .collect();
        on.sort_by_key(|d| d.span.start);
        let mut pos = 0;
        for d in on {
            if d.span.start < pos || d.treatment == Treatment::Keep {
                continue;
            }
            out.push_str(&p.text[pos..d.span.start]);
            match (&d.treatment, &d.generalized) {
                (Treatment::Generalize, Some(g)) if !d.needs_review => out.push_str(g),
                _ => out.push_str("[INKED]"),
            }
            pos = d.span.end;
        }
        out.push_str(&p.text[pos..]);
        out.push('\u{c}');
    }
    out
}

/// The synthetic values typed on each IRS fixture that must never be visible.
const IRS_VALUES: &[(&str, &[&str])] = &[
    (
        "irs-1099-misc-2025.txt",
        &[
            "Adatum Property Management LLC",
            "40 River Rd",
            "(518) 555-0100",
            "78-0001234",
            "900-41-9900",
            "Sam T Sample",
            "7 Hill St",
            "AD-4410-22",
            "7700-1234",
        ],
    ),
    (
        "irs-1099-misc-2026.txt",
        &[
            "Adatum Property Management LLC",
            "40 River Rd",
            "(518) 555-0100",
            "78-0001234",
            "900-41-9900",
            "Sam T Sample",
            "7 Hill St",
            "AD-4410-22",
            "7700-1234",
        ],
    ),
    (
        "irs-1098-2025.txt",
        &[
            "Adatum Mortgage Co",
            "1 Lender Plaza",
            "(520) 555-0111",
            "17-2233445",
            "912-34-5678",
            "Maria Example-Lopez",
            "61 Willow Ct",
            "4471-0099-3311",
        ],
    ),
    (
        "irs-1099-int-2025.txt",
        &[
            "Northwind Credit Union",
            "77 Lakeview Ave",
            "(608) 555-0199",
            "17-2233445",
            "912-34-5678",
            "John A Sample",
            "22 Birch Rd Unit 5",
            "5501-2299-87",
        ],
    ),
    (
        "irs-1099-nec-2025.txt",
        &[
            "Fabrikam Consulting Inc",
            "900 Market St Fl 3",
            "28-1100220",
            "945-67-8901",
            "Pat R Example",
            "77 Oak Ave",
            "NEC-000417",
            "4471-0099",
        ],
    ),
    (
        "irs-w2-2025.txt",
        &[
            "900-14-2231",
            "07-4400123",
            "Contoso Widgets LLC",
            "500 Harbor Blvd Suite 200",
            "CW-0042-77",
            "Jane Q Doe",
            "1428 Elm Street Apt 4B",
            "0047-91234",
        ],
    ),
    (
        "irs-k1-1065-2025.txt",
        &[
            "78-0004455",
            "Coho Partners LP",
            "45 Vineyard Rd",
            "900-12-6655",
            "Lee M Example",
            "14 Cedar Ln",
        ],
    ),
    (
        "irs-k1-1120s-2025.txt",
        &[
            "89-0003344",
            "Wingtip Design Group",
            "8 Mill St",
            "900-55-1212",
            "Robin K Sample",
            "3 Pine Ct",
        ],
    ),
    (
        "irs-1095-b-2025.txt",
        &[
            "Pat R Example",
            "945-67-8901",
            "77 Oak Ave",
            "Northwind Health Plan",
            "89-0001122",
            "(208) 555-0142",
            "900 Main St",
        ],
    ),
    (
        "irs-1095-c-2025.txt",
        &[
            "Jane Q Doe",
            "900-14-2231",
            "1428 Elm Street Apt 4B",
            "Contoso Widgets LLC",
            "07-4400123",
            "500 Harbor Blvd Suite 200",
            "(217) 555-0100",
        ],
    ),
    (
        "irs-1099-b-2025.txt",
        &[
            "Tailspin Brokerage LLC",
            "1 Commerce Plaza",
            "(617) 555-0123",
            "78-5500660",
            "900-22-3344",
            "Alex Mock",
            "14 Cedar Ln",
            "TB-5520-0091",
        ],
    ),
];

/// At most this many attention questions on one IRS form.
const QUESTION_BUDGET: usize = 3;

#[test]
fn no_typed_value_is_visible_on_irs_layouts() {
    let mut leaks = Vec::new();
    for (file, values) in IRS_VALUES {
        let (e, dets, _) = run(file);
        let seen = visible(&e, &dets);
        leaks.extend(
            values
                .iter()
                .filter(|v| seen.contains(**v))
                .map(|v| (file, v)),
        );
    }
    assert!(leaks.is_empty(), "visible: {leaks:?}");
}

#[test]
fn irs_layouts_stay_within_the_question_budget() {
    let mut over = Vec::new();
    for (file, _) in IRS_VALUES {
        let (e, dets, _) = run(file);
        let asked: Vec<&Detection> = dets.iter().filter(|d| d.needs_review).collect();
        for d in &asked {
            let v = value(&e, d);
            let p = d.review_prompt.as_deref().unwrap_or_default();
            assert!(!p.contains(v.trim()), "{file}: prompt shows {v:?}");
        }
        if asked.len() > QUESTION_BUDGET {
            over.push((
                file,
                asked
                    .iter()
                    .map(|d| value(&e, d).to_string())
                    .collect::<Vec<_>>(),
            ));
        }
    }
    assert!(over.is_empty(), "over budget: {over:#?}");
}

/// The text of every match of template field `id`'s label on the form pages,
/// whitespace collapsed.
fn label_texts(e: &Extracted, id: &str) -> Vec<String> {
    let set = packs();
    let m = template::match_template(e, &set).expect("a template");
    let f = m
        .template
        .fields
        .iter()
        .find(|f| f.id == id)
        .unwrap_or_else(|| panic!("no field {id}"));
    e.pages
        .iter()
        .filter(|p| m.pages.contains(&p.page))
        .flat_map(|p| {
            f.label
                .find_iter(&p.text)
                .map(|l| l.as_str().split_whitespace().collect::<Vec<_>>().join(" "))
                .collect::<Vec<_>>()
        })
        .collect()
}

/// A line break after each phrase, as IRS layouts wrap labels mid-phrase.
fn wrap_after(text: &str, phrases: &[&str]) -> String {
    let mut t = text.to_string();
    for p in phrases {
        let w = t.replacen(&format!("{p} "), &format!("{p} \n"), 1);
        assert_ne!(t, w, "{p} not in fixture");
        t = w;
    }
    t
}

/// One IRS text layer: its kind and year, the first typed value, values
/// that must be inked, labels that must match in full, and where to wrap.
struct IrsForm {
    file: &'static str,
    kind: &'static str,
    year: u16,
    first_value: &'static str,
    inked: &'static [&'static str],
    labels: &'static [(&'static str, &'static str)],
    wraps: &'static [&'static str],
}

fn check_irs_form(f: &IrsForm) {
    let text = fixture(f.file);
    for (variant, t) in [
        ("as extracted", text.clone()),
        ("wrapped", wrap_after(&text, f.wraps)),
    ] {
        let e = pages(&t);
        let (dets, c) = detect(&e, &packs()).unwrap();
        assert_eq!(
            (c.kind.as_str(), c.year),
            (f.kind, Some(f.year)),
            "{} {variant}",
            f.file
        );
        template_values_only(&e, &dets, f.first_value);
        inked(&e, &dets, f.inked);
        for (id, want) in f.labels {
            assert_eq!(
                label_texts(&e, id).first().map(String::as_str),
                Some(*want),
                "{} {variant}: label of {id}",
                f.file
            );
        }
    }
}

#[test]
fn k1_1065_labels_are_read_in_full() {
    check_irs_form(&IrsForm {
        file: "irs-k1-1065-2025.txt",
        kind: "k1-1065",
        year: 2025,
        first_value: "78-0004455",
        inked: &["78-0004455", "900-12-6655", "Coho Partners LP"],
        labels: &[
            (
                "D",
                "D Check if this is a publicly traded partnership (PTP)",
            ),
            (
                "E",
                "E Partner’s SSN or TIN (Do not use TIN of a disregarded entity. See instructions.)",
            ),
            (
                "F_name",
                "F Name, address, city, state, and ZIP code for partner entered in E. See instructions.",
            ),
            (
                "H2_tin",
                "H2 If the partner is a disregarded entity (DE), enter the partner’s: TIN Name",
            ),
            (
                "G",
                "G General partner or LLC member-manager Limited partner or other LLC member",
            ),
            ("H1", "H1 Domestic partner Foreign partner"),
            (
                "J",
                "J Partner’s share of profit, loss, and capital (see instructions):",
            ),
            (
                "M",
                "M Did the partner contribute property with a built-in gain (loss)?",
            ),
            (
                "N",
                "N Partner’s Share of Net Unrecognized Section 704(c) Gain or (Loss)",
            ),
        ],
        wraps: &["TIN of a", "for partner", "entity (DE),", "loss, and"],
    });
}

#[test]
fn k1_1120s_labels_are_read_in_full() {
    check_irs_form(&IrsForm {
        file: "irs-k1-1120s-2025.txt",
        kind: "k1-1120s",
        year: 2025,
        first_value: "89-0003344",
        inked: &["89-0003344", "900-55-1212"],
        labels: &[
            ("E", "E Shareholder’s identifying number"),
            ("F3", "F3 What type of entity is this shareholder?"),
            (
                "F2_tin",
                "F2 If the shareholder is a disregarded entity, a trust, an estate, or a nominee or similar person, enter the individual or entity responsible for reporting: TIN Name",
            ),
        ],
        wraps: &["a trust,", "entity responsible"],
    });
}

#[test]
fn f1095_b_labels_are_read_in_full() {
    check_irs_form(&IrsForm {
        file: "irs-1095-b-2025.txt",
        kind: "1095-b",
        year: 2025,
        first_value: "Pat R Example",
        inked: &["945-67-8901", "89-0001122", "(208) 555-0142"],
        labels: &[
            (
                "1",
                "1 Name of responsible individual–First name, middle name, last name",
            ),
            (
                "3",
                "3 Date of birth (if SSN or other TIN is not available)",
            ),
            (
                "8",
                "8 Enter letter identifying Origin of the Health Coverage (see instructions for codes):",
            ),
            (
                "privacy_notice",
                "For Privacy Act and Paperwork Reduction Act Notice, see separate instructions.",
            ),
        ],
        wraps: &["middle name,", "or other TIN", "Health Coverage"],
    });
}

#[test]
fn f1095_c_labels_are_read_in_full() {
    check_irs_form(&IrsForm {
        file: "irs-1095-c-2025.txt",
        kind: "1095-c",
        year: 2025,
        first_value: "Jane Q Doe",
        inked: &["900-14-2231", "07-4400123", "Contoso Widgets LLC"],
        labels: &[
            (
                "1",
                "1 Name of employee (first name, middle initial, last name)",
            ),
            (
                "privacy_notice",
                "For Privacy Act and Paperwork Reduction Act Notice, see separate instructions.",
            ),
        ],
        wraps: &["(first name,"],
    });
}

#[test]
fn f1099_b_labels_are_read_in_full() {
    check_irs_form(&IrsForm {
        file: "irs-1099-b-2025.txt",
        kind: "1099-b",
        year: 2025,
        first_value: "Tailspin Brokerage",
        inked: &[
            "78-5500660",
            "900-22-3344",
            "Tailspin Brokerage LLC",
            "TB-5520-0091",
        ],
        labels: &[
            (
                "payer_name",
                "PAYER’S name, street address, city or town, state or province, country, ZIP or foreign postal code, and telephone no.",
            ),
            (
                "recipient_city",
                "City or town, state or province, country, and ZIP or foreign postal code",
            ),
            (
                "2",
                "2 Short-term gain or loss Long-term gain or loss Ordinary",
            ),
            ("3", "3 If checked, proceeds from:"),
            ("6", "6 Reported to IRS:"),
            (
                "7",
                "7 If checked, loss is not allowed based on amount in 1d",
            ),
            (
                "9",
                "9 Unrealized profit or (loss) on open contracts—12/31/2024",
            ),
            (
                "10",
                "10 Unrealized profit or (loss) on open contracts—12/31/2025",
            ),
        ],
        wraps: &["province,", "loss is not"],
    });
}

/// The person named on each IRS fixture.
const IRS_NAMES: &[(&str, &str)] = &[
    ("irs-1099-misc-2025.txt", "Sam T Sample"),
    ("irs-1099-misc-2026.txt", "Sam T Sample"),
    ("irs-1098-2025.txt", "Maria Example-Lopez"),
    ("irs-1099-int-2025.txt", "John A Sample"),
    ("irs-1099-nec-2025.txt", "Pat R Example"),
    ("irs-w2-2025.txt", "Jane Q Doe"),
    ("irs-k1-1065-2025.txt", "Lee M Example"),
    ("irs-k1-1120s-2025.txt", "Robin K Sample"),
    ("irs-1095-b-2025.txt", "Pat R Example"),
    ("irs-1095-c-2025.txt", "Jane Q Doe"),
    ("irs-1099-b-2025.txt", "Alex Mock"),
];

#[test]
fn no_part_of_a_typed_name_is_visible_on_irs_layouts() {
    // Not just the whole name: a template or stage that reads part of a
    // name line must not leave the rest (a surname) beside its token.
    let mut leaks = Vec::new();
    for (file, name) in IRS_NAMES {
        let (e, dets, _) = run(file);
        let seen = visible(&e, &dets);
        let original = &e.pages.iter().map(|p| p.text.as_str()).collect::<String>();
        for part in name.split([' ', '-']).filter(|w| w.len() >= 3) {
            // Count whole-word occurrences, minus those that are form
            // wording in the blank IRS layout (before the typed values).
            let words = |t: &str| {
                t.split(|c: char| !c.is_alphanumeric())
                    .filter(|w| *w == part)
                    .count()
            };
            let typed_at = original.find(name).unwrap_or(original.len());
            let in_form = words(&original[..typed_at]);
            if words(&seen) > in_form {
                leaks.push((file, part));
            }
        }
    }
    assert!(leaks.is_empty(), "name parts visible: {leaks:?}");
}

#[test]
fn a_list_of_information_returns_does_not_match_a_template() {
    // Form W-9's instructions list the returns a requester files.
    let e = pages(
        "Examples of information returns include, but are not limited to, the following.\n\
         - Form 1099-INT (interest earned or paid).\n\
         - Form 1099-DIV (dividends, including those from stocks or mutual funds).\n\
         - Form 1099-MISC (various types of income, prizes, awards, or gross proceeds).\n\
         - Form 1099-NEC (nonemployee compensation).\n\
         - Form 1099-B (stock or mutual fund sales and certain other transactions by brokers).\n\
         - Form 1099-K (merchant card and third-party network transactions).\n\
         - Form 1098 (home mortgage interest), 1098-E (student loan interest), and 1098-T (tuition).\n",
    );
    let set = packs();
    let m = template::match_template(&e, &set);
    assert!(m.is_none(), "{:?}", m.map(|m| m.template.id.clone()));
}

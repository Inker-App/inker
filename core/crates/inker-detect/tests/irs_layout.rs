//! The IRS's own text layers: titles and labels wrap over lines, every label
//! comes first and the typed values after (as a flattened PDF reads), the
//! revision date sits next to the title, and a cover page talks about other
//! forms. Fixtures `irs-*.txt` are the IRS Copy B text with synthetic values
//! appended (SSN areas 9xx, unissued EIN prefixes, invented names); a form
//! feed separates pages.

#![allow(clippy::unwrap_used, clippy::expect_used)] // tests may panic

use std::path::PathBuf;

use inker_core::{Detection, DetectionSource, Extracted, PageText, TextOrigin};
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

//! US street address finder. Full addresses are generalized to "City, ST";
//! a bare "City, ST ZIP" line keeps city and state and drops the ZIP.

use std::sync::LazyLock;

use inker_core::{Detection, PageText, TokenKind, Treatment};
use regex::Regex;

use crate::util::{detection, overlaps, re, rule};

/// USPS state and territory abbreviations, plus the military "states".
pub const STATES: &[&str] = &[
    "AL", "AK", "AZ", "AR", "CA", "CO", "CT", "DE", "FL", "GA", "HI", "ID", "IL", "IN", "IA", "KS",
    "KY", "LA", "ME", "MD", "MA", "MI", "MN", "MS", "MO", "MT", "NE", "NV", "NH", "NJ", "NM", "NY",
    "NC", "ND", "OH", "OK", "OR", "PA", "RI", "SC", "SD", "TN", "TX", "UT", "VT", "VA", "WA", "WV",
    "WI", "WY", "DC", "PR", "VI", "GU", "AS", "MP", "AA", "AE", "AP",
];

const SUFFIXES: &str = "Street|St|Avenue|Ave|Road|Rd|Boulevard|Blvd|Drive|Dr|Lane|Ln|Court|Ct|\
    Place|Pl|Way|Terrace|Ter|Parkway|Pkwy|Highway|Hwy|Circle|Cir|Square|Sq|Trail|Trl|Loop|Row|\
    Plaza|Plz|Alley|Aly|Crescent|Pike|Run|Path|Walk";

const UNIT_WORDS: &str = "Apt|Apartment|Unit|Suite|Ste|Fl|Floor|Rm|Room";

const DIRECTIONS: &str = "North|South|East|West";

/// `words` ("Street|St") as written and in capitals ("STREET|ST"). Mailing
/// blocks on recipient copies and statements are printed in capitals; a
/// street word in lower case is prose, so it is not accepted.
fn either_case(words: &str) -> String {
    format!("{words}|{}", words.to_ascii_uppercase())
}

fn city_line() -> String {
    format!(
        r"(?P<city>[A-Z][A-Za-z.'-]*(?:[ ][A-Z][A-Za-z.'-]*){{0,3}}),?[ \t]+(?P<st>{})[ \t]+(?P<zip>\d{{5}}(?:-\d{{4}})?)\b",
        STATES.join("|")
    )
}

/// A street line: number, words, a street suffix; or a PO box.
fn street() -> String {
    let suffixes = either_case(SUFFIXES);
    let directions = either_case(DIRECTIONS);
    format!(
        r"(?:(?:P\.?[ ]?O\.?|Post[ ]Office|POST[ ]OFFICE)[ ]+(?:Box|BOX)[ ]+\d+|\d{{1,6}}[A-Z]?(?:[ ](?:{directions}|NE|NW|SE|SW|[NSEW]\.?))?(?:[ ][A-Z0-9][A-Za-z0-9'-]*){{1,4}}?[ ](?:{suffixes})\b\.?(?:[ ](?:NE|NW|SE|SW|[NSEW])\b)?)"
    )
}

/// An apartment, unit, suite or floor ("Apt 5", "UNIT 5B", "# 12").
fn unit() -> String {
    let words = either_case(UNIT_WORDS);
    format!(r"(?:(?:{words})\.?[ ]?#?|#)[ ]?[A-Za-z0-9-]+")
}

/// A street, its unit on the same line or the next, then its city line.
static FULL: LazyLock<Regex> = LazyLock::new(|| {
    let sep = r"(?:[ \t]*,[ \t]*|[ \t]*\r?\n[ \t]*|[ \t]+)";
    let unit = unit();
    re(&format!(
        r"\b{}(?:,?[ ]{unit})?(?:[ \t]*\r?\n[ \t]*{unit})?{sep}{}",
        street(),
        city_line()
    ))
});

static STREET_LINE: LazyLock<Regex> =
    LazyLock::new(|| re(&format!(r"^{}(?:,?[ ]{})?$", street(), unit())));

/// True when `s` is, in full, a street line with no city ("77 Oak Ave",
/// "500 Harbor Blvd Suite 200"), as forms print it in a box of its own.
pub fn is_street_line(s: &str) -> bool {
    STREET_LINE.is_match(s)
}

static CITY_LINE: LazyLock<Regex> = LazyLock::new(|| re(&format!(r"\b{}", city_line())));

fn city_state(c: &regex::Captures<'_>) -> Option<String> {
    Some(format!(
        "{}, {}",
        c.name("city")?.as_str(),
        c.name("st")?.as_str()
    ))
}

/// Every address on one page, as `Treatment::Generalize` detections.
pub fn find(page: &PageText) -> Vec<Detection> {
    let text = page.text.as_str();
    let mut out = Vec::new();
    let mut taken = Vec::new();
    let mut add = |start: usize, end: usize, general: Option<String>, conf: f32, name: &str| {
        let mut d = detection(
            page.page,
            start,
            end,
            TokenKind::Address,
            rule(name),
            conf,
            Treatment::Generalize,
        );
        d.generalized = general;
        out.push(d);
    };
    for c in FULL.captures_iter(text) {
        let Some(m) = c.get(0) else { continue };
        taken.push((m.start(), m.end()));
        add(m.start(), m.end(), city_state(&c), 0.8, "address");
    }
    for c in CITY_LINE.captures_iter(text) {
        let Some(m) = c.get(0) else { continue };
        if overlaps(&taken, m.start(), m.end()) {
            continue;
        }
        add(m.start(), m.end(), city_state(&c), 0.7, "city_state_zip");
    }
    out.sort_by_key(|d| d.span.start);
    out
}

/// "City, ST" from the last city/state/ZIP line in `text`, if any.
pub fn generalize(text: &str) -> Option<String> {
    CITY_LINE
        .captures_iter(text)
        .last()
        .as_ref()
        .and_then(city_state)
}

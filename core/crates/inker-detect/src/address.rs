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

fn city_line() -> String {
    format!(
        r"(?P<city>[A-Z][A-Za-z.'-]*(?:[ ][A-Z][A-Za-z.'-]*){{0,3}}),?[ \t]+(?P<st>{})[ \t]+(?P<zip>\d{{5}}(?:-\d{{4}})?)\b",
        STATES.join("|")
    )
}

static FULL: LazyLock<Regex> = LazyLock::new(|| {
    let street = format!(
        r"(?:(?:P\.?[ ]?O\.?|Post[ ]Office)[ ]+Box[ ]+\d+|\d{{1,6}}[A-Z]?(?:[ ](?:North|South|East|West|NE|NW|SE|SW|[NSEW]\.?))?(?:[ ][A-Z0-9][A-Za-z0-9'-]*){{1,4}}?[ ](?:{SUFFIXES})\b\.?(?:[ ](?:NE|NW|SE|SW|[NSEW])\b)?)"
    );
    let unit = r"(?:,?[ ](?:(?:Apt|Apartment|Unit|Suite|Ste|Fl|Floor|Rm|Room)\.?[ ]?#?|#)[ ]?[A-Za-z0-9-]+)?";
    let sep = r"(?:[ \t]*,[ \t]*|[ \t]*\r?\n[ \t]*|[ \t]+)";
    re(&format!(r"\b{street}{unit}{sep}{}", city_line()))
});

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

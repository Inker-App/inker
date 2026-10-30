//! Checksum and format rules, ported as plain logic from python-stdnum
//! (LGPL, so ported rather than linked) plus the context rules from the spec.
//!
//! Every detection here has `DetectionSource::Rule` and `Treatment::Tokenize`.
//! Spans are byte offsets into the page text.

use std::sync::LazyLock;

use inker_core::{Detection, PageText, TokenKind, Treatment};
use regex::Regex;

use crate::util::{
    Folded, box_runs, detection, digits, isolated, label_before, overlaps, re, rule,
};

/// The outcome of checking nine digits as an SSN.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SsnCheck {
    /// A valid SSN.
    Valid,
    /// Not an SSN, but a valid ITIN (9xx with an ITIN group).
    Itin,
    /// Area 000, 666 or 9xx: never issued. Synthetic documents use these,
    /// and a value written in SSN format is still inked, at lower confidence.
    Reserved,
    /// Group 00, serial 0000, a known sample number, or not nine digits.
    Invalid,
}

/// Published sample SSNs (wallet inserts, ads) that are never real.
pub const SSN_SAMPLES: &[&str] = &["078051120", "219099999", "123456789"];

/// Nine digits from `AAA-GG-SSSS`, `AAA GG SSSS` or `AAAGGSSSS`; else `None`.
fn nine_digits_3_2_4(s: &str) -> Option<String> {
    let joined: String = s.chars().filter(|&c| c != '\n').collect();
    let b = joined.as_bytes();
    let ok = match b.len() {
        9 => b.iter().all(u8::is_ascii_digit),
        11 => {
            matches!(b[3], b'-' | b' ' | b'.')
                && b[6] == b[3]
                && b.iter()
                    .enumerate()
                    .all(|(i, c)| i == 3 || i == 6 || c.is_ascii_digit())
        }
        _ => false,
    };
    ok.then(|| digits(s))
}

fn itin_group(group: u32) -> bool {
    matches!(group, 50..=65 | 70..=88 | 90..=92 | 94..=99)
}

/// Classify nine digits (separators allowed) as an SSN.
pub fn ssn_check(s: &str) -> SsnCheck {
    let Some(d) = nine_digits_3_2_4(s) else {
        return SsnCheck::Invalid;
    };
    let num = |r: std::ops::Range<usize>| d[r].parse::<u32>().unwrap_or(0);
    let (area, group, serial) = (num(0..3), num(3..5), num(5..9));
    if area >= 900 {
        return if itin_group(group) {
            SsnCheck::Itin
        } else {
            SsnCheck::Reserved
        };
    }
    if area == 0 || area == 666 {
        return SsnCheck::Reserved;
    }
    if group == 0 || serial == 0 || SSN_SAMPLES.contains(&d.as_str()) {
        return SsnCheck::Invalid;
    }
    SsnCheck::Valid
}

/// True for a valid SSN: area not 000, 666 or 900-999; group not 00; serial
/// not 0000; not a published sample.
pub fn is_valid_ssn(s: &str) -> bool {
    ssn_check(s) == SsnCheck::Valid
}

/// True for a valid ITIN: starts with 9, group 50-65, 70-88, 90-92 or 94-99.
pub fn is_valid_itin(s: &str) -> bool {
    ssn_check(s) == SsnCheck::Itin
}

/// EIN prefixes the IRS has issued, with the campus that assigns them
/// (IRS, "How EINs are assigned and valid EIN prefixes"). Unissued:
/// 00, 07, 08, 09, 17, 18, 19, 28, 29, 49, 69, 70, 78, 79, 89, 96, 97.
pub const EIN_PREFIXES: &[(&str, &str)] = &[
    ("01", "Brookhaven"),
    ("02", "Brookhaven"),
    ("03", "Brookhaven"),
    ("04", "Brookhaven"),
    ("05", "Brookhaven"),
    ("06", "Brookhaven"),
    ("10", "Andover"),
    ("11", "Brookhaven"),
    ("12", "Andover"),
    ("13", "Brookhaven"),
    ("14", "Brookhaven"),
    ("15", "Fresno"),
    ("16", "Brookhaven"),
    ("20", "Internet"),
    ("21", "Brookhaven"),
    ("22", "Brookhaven"),
    ("23", "Brookhaven"),
    ("24", "Fresno"),
    ("25", "Brookhaven"),
    ("26", "Internet"),
    ("27", "Internet"),
    ("30", "Cincinnati"),
    ("31", "Small Business Administration"),
    ("32", "Cincinnati"),
    ("33", "Philadelphia"),
    ("34", "Brookhaven"),
    ("35", "Cincinnati"),
    ("36", "Cincinnati"),
    ("37", "Cincinnati"),
    ("38", "Cincinnati"),
    ("39", "Philadelphia"),
    ("40", "Kansas City"),
    ("41", "Philadelphia"),
    ("42", "Philadelphia"),
    ("43", "Philadelphia"),
    ("44", "Kansas City"),
    ("45", "Internet"),
    ("46", "Internet"),
    ("47", "Internet"),
    ("48", "Philadelphia"),
    ("50", "Austin"),
    ("51", "Brookhaven"),
    ("52", "Brookhaven"),
    ("53", "Austin"),
    ("54", "Brookhaven"),
    ("55", "Brookhaven"),
    ("56", "Brookhaven"),
    ("57", "Brookhaven"),
    ("58", "Brookhaven"),
    ("59", "Brookhaven"),
    ("60", "Atlanta"),
    ("61", "Cincinnati"),
    ("62", "Philadelphia"),
    ("63", "Philadelphia"),
    ("64", "Philadelphia"),
    ("65", "Brookhaven"),
    ("66", "Philadelphia"),
    ("67", "Atlanta"),
    ("68", "Philadelphia"),
    ("71", "Philadelphia"),
    ("72", "Philadelphia"),
    ("73", "Philadelphia"),
    ("74", "Philadelphia"),
    ("75", "Philadelphia"),
    ("76", "Philadelphia"),
    ("77", "Philadelphia"),
    ("80", "Ogden"),
    ("81", "Internet"),
    ("82", "Internet"),
    ("83", "Internet"),
    ("84", "Internet"),
    ("85", "Internet"),
    ("86", "Internet"),
    ("87", "Internet"),
    ("88", "Internet"),
    ("90", "Ogden"),
    ("91", "Philadelphia"),
    ("92", "Internet"),
    ("93", "Internet"),
    ("94", "Memphis"),
    ("95", "Memphis"),
    ("98", "International"),
    ("99", "Internet"),
];

/// Nine digits from `PP-NNNNNNN` or `PPNNNNNNN`; else `None`.
fn ein_digits(s: &str) -> Option<String> {
    let b = s.as_bytes();
    let ok = match b.len() {
        9 => b.iter().all(u8::is_ascii_digit),
        10 => {
            b[2] == b'-'
                && b.iter()
                    .enumerate()
                    .all(|(i, c)| i == 2 || c.is_ascii_digit())
        }
        _ => false,
    };
    ok.then(|| digits(s))
}

/// The issuing campus for an EIN's prefix, if the prefix is valid.
pub fn ein_campus(s: &str) -> Option<&'static str> {
    let d = ein_digits(s)?;
    EIN_PREFIXES
        .iter()
        .find(|(p, _)| d.starts_with(p))
        .map(|(_, c)| *c)
}

/// True for nine digits (`PP-NNNNNNN` or bare) with an issued prefix.
pub fn is_valid_ein(s: &str) -> bool {
    ein_campus(s).is_some()
}

/// True for an ABA routing number: nine digits, 3-7-1 weighted checksum,
/// Federal Reserve prefix 00-12, 21-32, 61-72 or 80; never all zeros.
pub fn is_valid_aba(s: &str) -> bool {
    if s.len() != 9 || !s.bytes().all(|c| c.is_ascii_digit()) || s.bytes().all(|c| c == b'0') {
        return false;
    }
    let d: Vec<u32> = s.bytes().map(|c| u32::from(c - b'0')).collect();
    let prefix = d[0] * 10 + d[1];
    if !matches!(prefix, 0..=12 | 21..=32 | 61..=72 | 80) {
        return false;
    }
    let sum: u32 = d
        .iter()
        .zip([3, 7, 1].iter().cycle())
        .map(|(a, w)| a * w)
        .sum();
    sum.is_multiple_of(10)
}

/// Luhn mod-10 over the digits of `s` (other characters are ignored).
pub fn luhn(s: &str) -> bool {
    let d = digits(s);
    if d.is_empty() {
        return false;
    }
    let sum: u32 = d
        .bytes()
        .rev()
        .enumerate()
        .map(|(i, c)| {
            let n = u32::from(c - b'0');
            if i % 2 == 1 {
                let x = n * 2;
                if x > 9 { x - 9 } else { x }
            } else {
                n
            }
        })
        .sum();
    sum.is_multiple_of(10)
}

/// Card brand by IIN range, if the length fits the brand.
pub fn card_brand(digits: &str) -> Option<&'static str> {
    let len = digits.len();
    let p = |n: usize| {
        digits
            .get(..n)
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(0)
    };
    let brand = if matches!(p(2), 34 | 37) {
        (len == 15).then_some("amex")
    } else if p(1) == 4 {
        matches!(len, 13 | 16 | 19).then_some("visa")
    } else if matches!(p(2), 51..=55) || matches!(p(4), 2221..=2720) {
        (len == 16).then_some("mastercard")
    } else if p(4) == 6011
        || matches!(p(3), 644..=649)
        || p(2) == 65
        || matches!(p(6), 622126..=622925)
    {
        (16..=19).contains(&len).then_some("discover")
    } else if matches!(p(4), 2200..=2204) {
        (16..=19).contains(&len).then_some("mir")
    } else if matches!(p(4), 3528..=3589) {
        (16..=19).contains(&len).then_some("jcb")
    } else if matches!(p(3), 300..=305) || matches!(p(2), 36 | 38 | 39) {
        (14..=19).contains(&len).then_some("diners")
    } else if p(2) == 62 {
        (16..=19).contains(&len).then_some("unionpay")
    } else if matches!(p(2), 50 | 56..=58 | 67) {
        (13..=19).contains(&len).then_some("maestro")
    } else {
        None
    };
    brand.filter(|_| (13..=19).contains(&len))
}

/// True for a payment card: 13-19 digits, Luhn, a known IIN range.
pub fn is_valid_card(s: &str) -> bool {
    let d = digits(s);
    (13..=19).contains(&d.len())
        && s.bytes()
            .all(|c| c.is_ascii_digit() || c == b' ' || c == b'-')
        && luhn(&d)
        && card_brand(&d).is_some()
}

// ---------------------------------------------------------------------------
// Finding values in text
// ---------------------------------------------------------------------------

/// Labels that make a bare nine-digit run a routing candidate.
pub const ROUTING_LABELS: &[&str] = &["routing", "aba", "rtn", "r/t", "transit", "wire"];
/// Labels that make a bare nine-digit run an SSN or ITIN candidate.
pub const SSN_LABELS: &[&str] = &[
    "ssn",
    "social security",
    "soc sec",
    "ss#",
    "ss #",
    "itin",
    "tin",
    "taxpayer id",
    "taxpayer identification",
];
/// Labels that make a bare nine-digit run an EIN candidate.
pub const EIN_LABELS: &[&str] = &[
    "ein",
    "fein",
    "employer identification",
    "employer id",
    "federal id",
];
/// Labels that make a 6-17 digit run an account number.
pub const ACCOUNT_LABELS: &[&str] = &[
    "account",
    "acct",
    "a/c",
    "member",
    "policy",
    "loan",
    "mortgage",
    "account number",
];

/// Labels that make a 5-, 6- or 11-digit run a PIN: the e-file self-select
/// PIN and the third-party designee's PIN (five digits), an EFIN (six) and
/// the ERO's EFIN followed by its PIN (eleven).
pub const PIN_LABELS: &[&str] = &["pin", "efin", "personal identification number"];
/// Labels that make the run an Identity Protection PIN (six digits).
pub const IP_PIN_LABELS: &[&str] = &["identity protection pin", "ip pin"];
/// Digit counts a PIN comes in.
const PIN_LENGTHS: &[usize] = &[5, 6, 11];
/// How far after a PIN label its value may start: the rest of the label's
/// line, or the line below (a column head over its value).
const PIN_WINDOW: usize = 80;

static EMAIL: LazyLock<Regex> =
    LazyLock::new(|| re(r"[A-Za-z0-9._%+-]+@[A-Za-z0-9-]+(?:\.[A-Za-z0-9-]+)*\.[A-Za-z]{2,}"));
static CARD: LazyLock<Regex> = LazyLock::new(|| re(r"\d(?:[ -]?\d){12,18}"));
/// `AAA-GG-SSSS` (a line break may follow a dash), `AAA.GG.SSSS`, `AAA GG SSSS`.
static SSN_FMT: LazyLock<Regex> =
    LazyLock::new(|| re(r"\d{3}-\n?\d{2}-\n?\d{4}|\d{3}\.\n?\d{2}\.\n?\d{4}|\d{3} \d{2} \d{4}"));
static NINE_SPACED: LazyLock<Regex> = LazyLock::new(|| re(r"\d{3} \d{3} \d{3}"));
static EIN_FMT: LazyLock<Regex> = LazyLock::new(|| re(r"\d{2}-\d{7}"));
static PHONE: LazyLock<Regex> =
    LazyLock::new(|| re(r"(?:\+?1[ .-]?)?(?:\((\d{3})\) ?|(\d{3})[ .-])(\d{3})[ .-]\d{4}"));
/// A bare ten-digit North American number: area and exchange start 2-9.
static PHONE_BARE: LazyLock<Regex> = LazyLock::new(|| re(r"[2-9]\d{2}[2-9]\d{6}"));
static NINE: LazyLock<Regex> = LazyLock::new(|| re(r"\d{9}"));
static DIGIT_RUN: LazyLock<Regex> = LazyLock::new(|| re(r"\d+"));
static DATE: LazyLock<Regex> = LazyLock::new(|| re(r"^\d{4}-\d{2}-\d{2}$|^\d{2}-\d{2}-\d{4}$"));

/// Context window, in bytes before a value, for the label rules.
const CONTEXT: usize = 30;

/// Run every rule over one page. Dash variants and fullwidth digits are
/// folded first; spans point into the original text.
pub fn find(page: &PageText) -> Vec<Detection> {
    let folded = Folded::new(&page.text);
    if folded.is_identity(&page.text) {
        return find_folded(page);
    }
    let fp = PageText {
        page: page.page,
        text: folded.text.clone(),
        origin: page.origin,
    };
    find_folded(&fp)
        .into_iter()
        .map(|mut d| {
            let (s, e) = folded.span_to_orig(&page.text, d.span.start, d.span.end);
            d.span.start = s;
            d.span.end = e;
            d
        })
        .collect()
}

fn find_folded(page: &PageText) -> Vec<Detection> {
    let text = page.text.as_str();
    let lower = text.to_ascii_lowercase();
    let mut sink = Sink {
        page: page.page,
        out: Vec::new(),
        taken: Vec::new(),
    };

    for m in EMAIL.find_iter(text) {
        sink.add(m.start(), m.end(), TokenKind::Email, "email", 0.95);
    }

    for m in SSN_FMT.find_iter(text) {
        if !sink.free(text, m.start(), m.end()) {
            continue;
        }
        let (kind, conf, name) = match ssn_check(m.as_str()) {
            SsnCheck::Valid => (TokenKind::Ssn, 0.95, "ssn"),
            SsnCheck::Itin => (TokenKind::Itin, 0.95, "itin"),
            SsnCheck::Reserved => (TokenKind::Ssn, 0.55, "ssn_reserved"),
            SsnCheck::Invalid => continue,
        };
        sink.add(m.start(), m.end(), kind, name, conf);
    }

    for m in EIN_FMT.find_iter(text) {
        if !sink.free(text, m.start(), m.end()) {
            continue;
        }
        let conf = if is_valid_ein(m.as_str()) { 0.92 } else { 0.5 };
        sink.add(m.start(), m.end(), TokenKind::Ein, "ein", conf);
    }

    for m in CARD.find_iter(text) {
        if !sink.free(text, m.start(), m.end()) {
            continue;
        }
        if is_valid_card(m.as_str()) {
            sink.add(m.start(), m.end(), TokenKind::Card, "card", 0.9);
        }
    }

    for c in PHONE.captures_iter(text) {
        let Some(m) = c.get(0) else { continue };
        if !sink.free(text, m.start(), m.end()) {
            continue;
        }
        let area = c.get(1).or_else(|| c.get(2)).map_or("", |a| a.as_str());
        let exchange = c.get(3).map_or("", |a| a.as_str());
        let ok = |s: &str| {
            s.as_bytes()
                .first()
                .is_some_and(|b| (b'2'..=b'9').contains(b))
        };
        if ok(area) && ok(exchange) {
            sink.add(m.start(), m.end(), TokenKind::Phone, "phone", 0.85);
        }
    }

    // Routing numbers written in three groups, after a routing label.
    for m in NINE_SPACED.find_iter(text) {
        let (s, e) = (m.start(), m.end());
        if sink.free(text, s, e)
            && label_before(&lower, s, CONTEXT, ROUTING_LABELS).is_some()
            && is_valid_aba(&digits(m.as_str()))
        {
            sink.add(s, e, TokenKind::Routing, "aba_routing", 0.9);
        }
    }

    // Bare nine digits: routing first, then SSN/ITIN, then EIN, by context.
    for m in NINE.find_iter(text) {
        let (s, e) = (m.start(), m.end());
        if !sink.free(text, s, e) {
            continue;
        }
        let v = m.as_str();
        let near = |labels: &[&str]| label_before(&lower, s, CONTEXT, labels).is_some();
        if near(ROUTING_LABELS) && is_valid_aba(v) {
            sink.add(s, e, TokenKind::Routing, "aba_routing", 0.9);
        } else if near(SSN_LABELS) && ssn_check(v) != SsnCheck::Invalid {
            let (kind, conf) = match ssn_check(v) {
                SsnCheck::Valid => (TokenKind::Ssn, 0.6),
                SsnCheck::Itin => (TokenKind::Itin, 0.6),
                _ => (TokenKind::Ssn, 0.45),
            };
            sink.add(s, e, kind, "ssn_context", conf);
        } else if near(EIN_LABELS) && is_valid_ein(v) {
            sink.add(s, e, TokenKind::Ein, "ein_context", 0.6);
        }
    }

    // PINs: a whole 5-, 6- or 11-digit run after a PIN label on the same
    // line or the line above.
    for m in DIGIT_RUN.find_iter(text) {
        let (s, e) = (m.start(), m.end());
        if !PIN_LENGTHS.contains(&(e - s)) || !sink.free(text, s, e) {
            continue;
        }
        let near = |labels: &[&str]| {
            label_before(&lower, s, PIN_WINDOW, labels)
                .filter(|&d| text[s - d..s].matches('\n').count() <= 1)
        };
        let (pin, ip) = (near(PIN_LABELS), near(IP_PIN_LABELS));
        match (pin, ip) {
            (_, Some(i)) if pin.is_none_or(|p| i <= p) => {
                sink.add(
                    s,
                    e,
                    TokenKind::Custom("ip_pin".into()),
                    "ip_pin_context",
                    0.85,
                );
            }
            (Some(_), _) => {
                sink.add(s, e, TokenKind::Custom("pin".into()), "pin_context", 0.85);
            }
            _ => {}
        }
    }

    // Account numbers: 6-17 digits after an account label, digit boxes
    // ("0 0 0 1 2 3 ...", "0001 2345 6789") taken whole.
    for (s, e) in box_runs(text) {
        let v = &text[s..e];
        if !sink.free(text, s, e) || DATE.is_match(v) {
            continue;
        }
        let n = digits(v).len();
        if !(6..=17).contains(&n) {
            continue;
        }
        if text[..s].ends_with('$') {
            continue;
        }
        let Some(dist) = label_before(&lower, s, 60, ACCOUNT_LABELS) else {
            continue;
        };
        let conf = match dist {
            0..=16 => 0.85,
            17..=32 => 0.7,
            _ => 0.55,
        };
        sink.add(s, e, TokenKind::Account, "account_context", conf);
    }

    // Bare ten digits shaped like a phone number, after every labelled rule.
    for m in PHONE_BARE.find_iter(text) {
        if sink.free(text, m.start(), m.end()) {
            sink.add(m.start(), m.end(), TokenKind::Phone, "phone_bare", 0.6);
        }
    }

    let mut out = sink.out;
    out.sort_by_key(|d| d.span.start);
    out
}

/// Collects rule detections and the spans they claim, so later rules skip
/// text an earlier, more specific rule already took.
struct Sink {
    page: u32,
    out: Vec<Detection>,
    taken: Vec<(usize, usize)>,
}

impl Sink {
    fn add(&mut self, start: usize, end: usize, kind: TokenKind, name: &str, confidence: f32) {
        self.taken.push((start, end));
        self.out.push(detection(
            self.page,
            start,
            end,
            kind,
            rule(name),
            confidence,
            Treatment::Tokenize,
        ));
    }

    /// Not claimed yet and not glued to a longer number or word.
    fn free(&self, text: &str, start: usize, end: usize) -> bool {
        !overlaps(&self.taken, start, end) && isolated(text, start, end)
    }
}

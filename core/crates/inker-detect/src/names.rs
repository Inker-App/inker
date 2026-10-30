//! Conservative proper-name finder for free text. No ML: capitalised word
//! runs of 2-4 tokens after labels like "Recipient's name", plus a stop list.
//! An organisation is a run ending in a suffix (Inc, LLC, Bank, ...) or a
//! run under a payer, employer or lender label. Organisations ending in a
//! suffix are also found without a label. Everything here is low
//! confidence; the fail-closed pass and the attention card cover the rest.

use std::sync::LazyLock;

use inker_core::{Detection, PageText, TokenKind, Treatment};
use regex::Regex;

use crate::util::{detection, overlaps, re, rule};

/// Labels that introduce a name: (lowercase label, role, org by default).
pub const NAME_LABELS: &[(&str, Option<&str>, bool)] = &[
    ("employee's name", Some("employee"), false),
    ("employee name", Some("employee"), false),
    ("recipient's name", Some("recipient"), false),
    ("recipient name", Some("recipient"), false),
    ("payer's name", Some("payer"), true),
    ("payer name", Some("payer"), true),
    ("employer's name", Some("employer"), true),
    ("employer name", Some("employer"), true),
    ("lender's name", Some("lender"), true),
    ("lender", Some("lender"), true),
    ("borrower's name", Some("borrower"), false),
    ("borrower", Some("borrower"), false),
    ("prepared for", None, false),
    ("account holder", None, false),
    ("seller", None, false),
    ("buyer", None, false),
    ("taxpayer's name", None, false),
    ("spouse's name", None, false),
];

/// Words that end an organisation's name. Multi-word ones are matched as a
/// sequence of tokens.
const ORG_SUFFIXES: &[&str] = &[
    "inc",
    "inc.",
    "incorporated",
    "llc",
    "l.l.c.",
    "llp",
    "lp",
    "l.p.",
    "ltd",
    "ltd.",
    "corp",
    "corp.",
    "corporation",
    "company",
    "co.",
    "bank",
    "trust",
    "n.a.",
    "union",
];

const STOP_WORDS: &[&str] = &[
    // Months.
    "january",
    "february",
    "march",
    "april",
    "may",
    "june",
    "july",
    "august",
    "september",
    "october",
    "november",
    "december",
    "jan",
    "feb",
    "mar",
    "apr",
    "jun",
    "jul",
    "aug",
    "sep",
    "sept",
    "oct",
    "nov",
    "dec",
    // Form and statement words.
    "form",
    "forms",
    "box",
    "copy",
    "statement",
    "wage",
    "wages",
    "tax",
    "taxes",
    "total",
    "department",
    "treasury",
    "internal",
    "revenue",
    "service",
    "employee",
    "employer",
    "recipient",
    "payer",
    "name",
    "address",
    "street",
    "city",
    "state",
    "zip",
    "account",
    "number",
    "social",
    "security",
    "federal",
    "income",
    "interest",
    "dividends",
    "omb",
    "void",
    "corrected",
    "page",
    "date",
    "period",
    "summary",
    "balance",
    "checking",
    "savings",
    "deposits",
    "withdrawals",
    "closing",
    "disclosure",
    "settlement",
    "loan",
    "mortgage",
    "information",
    "instructions",
    "for",
    "the",
    "see",
    "part",
    "schedule",
    "line",
    "amount",
    "paid",
    "due",
    "code",
    "control",
    "last",
    "first",
    "suff",
    "initial",
    "identification",
    "tin",
    "ssn",
    "ein",
    "itin",
    "routing",
    "member",
    "fdic",
    "lender",
    "borrower",
    "buyer",
    "seller",
    "prepared",
    "phone",
    "telephone",
    "email",
    "fax",
    "website",
    "customer",
    "notice",
    "important",
    "year",
    "annual",
    "report",
    "return",
    "medicare",
    "retirement",
    "plan",
    "other",
    "local",
    "locality",
    "nonemployee",
    "compensation",
    "proceeds",
    "broker",
    "health",
    "coverage",
    "marketplace",
    "insurance",
    "partner",
    "shareholder",
    "partnership",
    "your",
    "our",
    "this",
    "from",
    "to",
    "and",
    "or",
    "of",
    "no",
    "yes",
    "not",
    "total",
    "subtotal",
    "receipt",
    "invoice",
    "wire",
    "transfer",
    "deposit",
    "payment",
    "payments",
    "fee",
    "fees",
    "charges",
    "credit",
    "debit",
    "card",
    "beginning",
    "ending",
    "opening",
    "new",
    "old",
    // Agencies the forms and their instructions name ("Internal Revenue
    // Service", "Department of the Treasury" and "Social Security" are
    // covered above). Never part of a person's name.
    "irs",
    "ssa",
    "tigta",
    "ftc",
    "tas",
    "fincen",
    "taxpayer",
    "advocate",
    "administration",
    "commission",
    "inspector",
];

/// Words that may open an organisation's run without being part of its name
/// ("An LLC", "Each LLC"). Shed like stop words, so a suffix with only these
/// before it is no organisation. Kept apart from the stop words because a
/// person's run must not stop at the initial "A".
const ORG_LEADS: &[&str] = &["a", "an", "any", "each", "every", "such", "another"];

/// A word shed from the front of an organisation's run.
fn is_org_lead(word: &str) -> bool {
    (is_stop_word(word) || ORG_LEADS.contains(&normalise(word).as_str())) && !is_suffix(word)
}

const STATE_NAMES: &[&str] = &[
    "alabama",
    "alaska",
    "arizona",
    "arkansas",
    "california",
    "colorado",
    "connecticut",
    "delaware",
    "florida",
    "georgia",
    "hawaii",
    "idaho",
    "illinois",
    "indiana",
    "iowa",
    "kansas",
    "kentucky",
    "louisiana",
    "maine",
    "maryland",
    "massachusetts",
    "michigan",
    "minnesota",
    "mississippi",
    "missouri",
    "montana",
    "nebraska",
    "nevada",
    "new hampshire",
    "new jersey",
    "new mexico",
    "new york",
    "north carolina",
    "north dakota",
    "ohio",
    "oklahoma",
    "oregon",
    "pennsylvania",
    "rhode island",
    "south carolina",
    "south dakota",
    "tennessee",
    "texas",
    "utah",
    "vermont",
    "virginia",
    "washington",
    "west virginia",
    "wisconsin",
    "wyoming",
    "district of columbia",
    "puerto rico",
];

/// Organisations ending in a suffix, anywhere on a line.
static ORG_RUN: LazyLock<Regex> = LazyLock::new(|| {
    re(
        r"(?:[A-Z][A-Za-z'’&.-]*[ ]){1,4}(?:Credit[ ]Union|Bank|Trust|Inc\.?|Incorporated|LLC|L\.L\.C\.|LLP|LP|L\.P\.|Ltd\.?|Corp\.?|Corporation|Company|Co\.|N\.A\.)(?:,[ ]N\.A\.)?(?:[^A-Za-z]|$)",
    )
});

/// A capitalised run found after a position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NameRun {
    pub start: usize,
    pub end: usize,
    /// Ends in an organisation suffix (Inc, LLC, Bank, ...).
    pub org: bool,
}

fn normalise(word: &str) -> String {
    let w = word.trim_end_matches([',', ':', ';']).to_ascii_lowercase();
    w.strip_suffix("'s")
        .or_else(|| w.strip_suffix("’s"))
        .unwrap_or(&w)
        .to_string()
}

/// True for a word that is never part of a name (months, form words, ...).
pub fn is_stop_word(word: &str) -> bool {
    let w = normalise(word);
    STOP_WORDS.contains(&w.as_str())
}

fn is_suffix(word: &str) -> bool {
    let w = word.trim_end_matches(',').to_ascii_lowercase();
    ORG_SUFFIXES.contains(&w.as_str())
}

fn is_capitalised(word: &str) -> bool {
    let w = word.trim_end_matches(',');
    w.chars().next().is_some_and(|c| c.is_ascii_uppercase())
        && w.chars()
            .all(|c| c.is_ascii_alphabetic() || "'’.-&".contains(c))
}

/// The capitalised run starting exactly at `start`, read to the end of its
/// column segment, if it is a plausible name (2-4 words, or 1-4 words plus an
/// organisation suffix, no stop words, not a state name).
pub fn run_at(text: &str, start: usize) -> Option<NameRun> {
    let line_end = text[start..].find('\n').map_or(text.len(), |i| start + i);
    let mut seg = &text[start..line_end];
    for brk in ["   ", "\t"] {
        if let Some(i) = seg.find(brk) {
            seg = &seg[..i];
        }
    }
    // Words with their byte offsets.
    let mut words: Vec<(usize, &str)> = Vec::new();
    let mut off = 0;
    for w in seg.split(' ') {
        if !w.is_empty() {
            words.push((start + off, w));
        }
        off += w.len() + 1;
    }
    // Take capitalised words (and connectors) until something else.
    let mut toks: Vec<(usize, &str)> = Vec::new();
    // The run is surname first ("EXAMPLE, JORDAN A").
    let mut surname_first = false;
    for (i, &(pos, w)) in words.iter().enumerate() {
        let connector = matches!(w, "&" | "of" | "and");
        if !(is_capitalised(w) || (connector && !toks.is_empty())) {
            break;
        }
        toks.push((pos, w));
        // A comma ends the run unless an org suffix follows, or it follows
        // a lone surname and a given name comes next.
        let next = words.get(i + 1).map(|&(_, n)| n);
        if w.ends_with(',') && !next.is_some_and(is_suffix) {
            if toks.len() == 1 && next.is_some_and(is_capitalised) {
                surname_first = true;
                continue;
            }
            break;
        }
    }
    // "Houston, TX" and "Austin, Texas" are a city and state, not a surname
    // and a given name.
    if surname_first
        && let (Some(&(given, _)), Some(&(last_pos, last))) = (toks.get(1), toks.last())
    {
        let rest = &text[given..last_pos + last.len()];
        if crate::address::STATES.contains(&rest)
            || STATE_NAMES.contains(&rest.to_ascii_lowercase().as_str())
        {
            return None;
        }
    }
    // An org ends at its last suffix and only sheds leading stop words and
    // articles ("Your Statement Northwind Federal Credit Union", "An LLC");
    // a suffix with no name left before it is no org. A person run is cut
    // at its first stop word and has no connectors.
    let org = match toks.iter().rposition(|(_, w)| is_suffix(w)) {
        Some(i) => {
            toks.truncate(i + 1);
            let lead = toks.iter().take_while(|(_, w)| is_org_lead(w)).count();
            toks.drain(..lead);
            true
        }
        None => {
            if let Some(i) = toks
                .iter()
                .position(|(_, w)| is_stop_word(w) || matches!(*w, "&" | "of" | "and"))
            {
                toks.truncate(i);
            }
            false
        }
    };
    let name_tokens = toks.iter().filter(|(_, w)| !is_suffix(w)).count();
    let ok = if org {
        (1..=4).contains(&name_tokens)
    } else {
        (2..=4).contains(&toks.len())
    };
    if !ok {
        return None;
    }
    // A person run that was longer than four words is not trusted at all.
    if !org
        && words
            .iter()
            .take_while(|(_, w)| is_capitalised(w) && !w.ends_with(','))
            .count()
            > 4
    {
        return None;
    }
    let (first, _) = *toks.first()?;
    let (last_pos, last) = *toks.last()?;
    let end = last_pos + last.trim_end_matches([',', ':', ';']).len();
    let joined = text[first..end].to_ascii_lowercase();
    if STATE_NAMES.contains(&joined.as_str()) {
        return None;
    }
    Some(NameRun {
        start: first,
        end,
        org,
    })
}

/// The capitalised run that starts after `pos` (skipping separators and at
/// most `window` bytes), or at the start of the next non-empty line.
pub fn name_after(text: &str, pos: usize, window: usize) -> Option<NameRun> {
    let b = text.as_bytes();
    let line_end = text[pos..].find('\n').map_or(text.len(), |i| pos + i);
    let mut i = pos;
    while i < line_end && matches!(b[i], b' ' | b'\t' | b':' | b'#' | b'-' | b',' | b'.') {
        i += 1;
    }
    if i < line_end
        && i - pos <= window
        && let Some(r) = run_at(text, i)
    {
        return Some(r);
    }
    // Same line had no name: try the next non-empty line.
    let mut j = line_end;
    while j < text.len() && b[j].is_ascii_whitespace() {
        j += 1;
    }
    if j > line_end && j < text.len() && text[line_end..j].contains('\n') {
        return run_at(text, j);
    }
    None
}

/// Names heading a mailing block: a line that is, in full, a person-shaped
/// capitalised run, directly above a street line ("48 Lantern Way Apt 3").
/// Statements print the holder's name this way with no label.
fn mailing_names(text: &str) -> Vec<(usize, usize)> {
    let mut lines: Vec<(usize, &str)> = Vec::new();
    let mut off = 0;
    for l in text.split('\n') {
        lines.push((off, l));
        off += l.len() + 1;
    }
    let mut out = Vec::new();
    for w in lines.windows(2) {
        let [(start, line), (_, next)] = w else {
            continue;
        };
        let lead = line.len() - line.trim_start().len();
        let body = line.trim();
        if body.is_empty() || !crate::address::is_street_line(next.trim()) {
            continue;
        }
        let Some(r) = run_at(text, start + lead) else {
            continue;
        };
        if !r.org && r.end - r.start == body.len() {
            out.push((r.start, r.end));
        }
    }
    out
}

/// Names after labels, names heading a mailing block, and organisations
/// ending in a suffix anywhere.
pub fn find(page: &PageText) -> Vec<Detection> {
    let text = page.text.as_str();
    let lower = text.to_ascii_lowercase();
    let mut out: Vec<Detection> = Vec::new();
    let mut taken: Vec<(usize, usize)> = Vec::new();
    for &(label, role, org_label) in NAME_LABELS {
        let curly = label.replace('\'', "’");
        for l in [label, curly.as_str()] {
            for (i, m) in lower.match_indices(l) {
                if i > 0 && lower.as_bytes()[i - 1].is_ascii_alphanumeric() {
                    continue;
                }
                let Some(r) = name_after(text, i + m.len(), 40) else {
                    continue;
                };
                if overlaps(&taken, r.start, r.end) {
                    continue;
                }
                let org = r.org || org_label;
                let (kind, conf, name) = if org {
                    (TokenKind::Org, 0.55, "org_label")
                } else {
                    (TokenKind::Person, 0.5, "name_label")
                };
                let mut d = detection(
                    page.page,
                    r.start,
                    r.end,
                    kind,
                    rule(name),
                    conf,
                    Treatment::Tokenize,
                );
                d.role = role.map(String::from);
                taken.push((r.start, r.end));
                out.push(d);
            }
        }
    }
    for (start, end) in mailing_names(text) {
        if overlaps(&taken, start, end) {
            continue;
        }
        taken.push((start, end));
        out.push(detection(
            page.page,
            start,
            end,
            TokenKind::Person,
            rule("mailing_block"),
            0.6,
            Treatment::Tokenize,
        ));
    }
    for m in ORG_RUN.find_iter(text) {
        // Drop leading stop words and the trailing non-letter the regex used as a boundary.
        let s = m.as_str();
        let body_end = m.start()
            + s.trim_end_matches(|c: char| !c.is_ascii_alphabetic() && c != '.')
                .len();
        let mut start = m.start();
        for w in text[m.start()..body_end].split(' ') {
            if is_org_lead(w) {
                start += w.len() + 1;
            } else {
                break;
            }
        }
        if start >= body_end || overlaps(&taken, start, body_end) {
            continue;
        }
        let Some(r) = run_at(text, start) else {
            continue;
        };
        if !r.org || overlaps(&taken, r.start, r.end) {
            continue;
        }
        taken.push((r.start, r.end));
        out.push(detection(
            page.page,
            r.start,
            r.end,
            TokenKind::Org,
            rule("org_suffix"),
            0.45,
            Treatment::Tokenize,
        ));
    }
    out.sort_by_key(|d| d.span.start);
    out
}

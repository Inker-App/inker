//! People in letters and agreements (added 2026-11-04). Correspondence and
//! contracts carry no labels like "Recipient's name"; their names sit in
//! fixed places instead: after "Dear", under a closing, over a title, on
//! "Name:", "By:" and "/s/" lines, after "Attention:", "c/o" and "with a
//! copy to:", before ", Esq.", on a cc line, before a defined term such as
//! `(the "Holder")`, and after an honorific. Each place gives a role:
//! addressee, signatory, contact or party.
//!
//! Conservative like the rest of the names pass: a candidate must be a
//! person-shaped run from [`names::run_at`] (2-4 capitalised words, no stop
//! words), and no word of it may be a job title, a generic addressee
//! ("Customer", "Team") or contract wording ("Agreement", "Merger").
//! Everything is low confidence; fail-closed and the attention card stay
//! behind it.

use std::sync::LazyLock;

use inker_core::{Detection, PageText, TokenKind, Treatment};
use regex::Regex;

use crate::names::{self, NameRun};
use crate::util::{detection, overlaps, re, rule};

/// Confidence of every person found here.
const CONFIDENCE: f32 = 0.5;

/// Words that make a line a job title ("Chief Executive Officer", "Director
/// of Procurement"). A title line needs at least one of them.
const TITLE_HEADS: &[&str] = &[
    "officer",
    "president",
    "director",
    "counsel",
    "partner",
    "manager",
    "secretary",
    "treasurer",
    "chair",
    "chairman",
    "chairwoman",
    "chairperson",
    "ceo",
    "cfo",
    "coo",
    "cto",
    "cio",
    "vp",
    "svp",
    "evp",
    "attorney",
    "principal",
    "founder",
    "co-founder",
    "controller",
    "comptroller",
    "associate",
    "owner",
    "trustee",
    "head",
    "advisor",
    "adviser",
    "consultant",
    "administrator",
    "coordinator",
    "representative",
    "supervisor",
    "specialist",
    "esq",
];

/// Words that never belong to a person's name in a letter or contract:
/// title words, generic addressees, closings and contract wording.
const NOT_NAME: &[&str] = &[
    // Title words beyond the heads.
    "chief",
    "vice",
    "executive",
    "general",
    "managing",
    "senior",
    "assistant",
    "deputy",
    "legal",
    "finance",
    "financial",
    "operations",
    "procurement",
    "human",
    "resources",
    "department",
    "office",
    "services",
    "team",
    "group",
    "board",
    "directors",
    "committee",
    "staff",
    // Generic addressees.
    "sir",
    "sirs",
    "madam",
    "client",
    "clients",
    "valued",
    "colleague",
    "colleagues",
    "friend",
    "friends",
    "all",
    "everyone",
    "shareholders",
    "stockholders",
    "investors",
    "partners",
    "neighbor",
    "neighbour",
    "resident",
    "homeowner",
    "applicant",
    "hiring",
    "whom",
    "concern",
    "care",
    "support",
    "file",
    // Closings.
    "sincerely",
    "regards",
    "truly",
    "yours",
    "respectfully",
    "cordially",
    "thanks",
    "thank",
    "best",
    // Contract wording.
    "agreement",
    "agreements",
    "plan",
    "merger",
    "option",
    "options",
    "cancellation",
    "company",
    "parent",
    "holder",
    "party",
    "parties",
    "effective",
    "section",
    "article",
    "exhibit",
    "notices",
    "purchase",
    "stock",
    "shares",
    "law",
    "laws",
    "act",
    "whereas",
    "witness",
    "whereof",
    "hereby",
];

/// Honorifics that may open a name ("Ms. Odalys Fenwick").
const HONORIFICS: &[&str] = &["mr", "ms", "mrs", "mx", "dr", "prof"];

/// Words shed from the front of a run that ends at a fixed place
/// ("and Ingrid Marchetti (the ...", "Between ...").
const LEADS: &[&str] = &[
    "between",
    "among",
    "and",
    "by",
    "with",
    "to",
    "from",
    "dear",
    "attention",
    "attn",
    "cc",
];

pub(crate) static SALUTATION: LazyLock<Regex> =
    LazyLock::new(|| re(r"(?m)^[ \t]*(?:Dear|DEAR)[ \t]+([^\n]*?)[ \t]*[,:][ \t]*$"));

pub(crate) static CLOSING: LazyLock<Regex> = LazyLock::new(|| {
    re(
        r"(?im)^[ \t]*(?:sincerely(?:[ ]yours)?|yours[ ](?:sincerely|truly|faithfully)|very[ ]truly[ ]yours|(?:best|kind|warm|warmest|with[ ]best)[ ]regards|regards|best[ ]wishes|respectfully(?:[ ]yours|[ ]submitted)?|cordially(?:[ ]yours)?|with[ ](?:thanks|appreciation)|best)[ \t]*,[ \t]*$",
    )
});

/// "Attention:", "Attn:", "c/o", "with a copy to:" (the label of a notices
/// block; the name follows on the same line or the next).
static NOTICE_LABEL: LazyLock<Regex> = LazyLock::new(|| {
    re(r"(?i)\b(?:attention[ \t]*:|attn\.?[ \t]*:|c/o\b|with[ ]a[ ]copy[ ]to[ \t]*:)")
});

static ESQ: LazyLock<Regex> = LazyLock::new(|| re(r",[ ]?(?:Esq|ESQ)\.?"));

static CC_LINE: LazyLock<Regex> = LazyLock::new(|| re(r"(?im)^[ \t]*b?cc[ \t]*:"));

/// A defined term opening: `(the "`, `("`, `(this “`.
static DEFINED: LazyLock<Regex> = LazyLock::new(|| re(r#"\((?:the[ ]|this[ ])?["“][A-Z]"#));

static HONORIFIC: LazyLock<Regex> =
    LazyLock::new(|| re(r"\b(?:Mr|Ms|Mrs|Mx|Dr|Prof|MR|MS|MRS|DR)\.[ ]+[A-Z]"));

static SLASH_S: LazyLock<Regex> = LazyLock::new(|| re(r"/s/[ ]*"));

/// A word as the lists hold it: lowercase, no trailing punctuation.
fn bare(word: &str) -> String {
    word.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '-')
        .to_ascii_lowercase()
}

fn in_list(word: &str, list: &[&str]) -> bool {
    list.contains(&bare(word).as_str())
}

/// The lines of `text` with their byte offsets.
fn lines(text: &str) -> Vec<(usize, &str)> {
    let mut out = Vec::new();
    let mut off = 0;
    for l in text.split('\n') {
        out.push((off, l));
        off += l.len() + 1;
    }
    out
}

/// The person-shaped run starting at `start`, without a trailing full stop
/// on its last word ("Mrs. Quayle."), if no word of it rules it out.
fn person_at(text: &str, start: usize) -> Option<(usize, usize)> {
    let NameRun { start, end, org } = names::run_at(text, start)?;
    if org {
        return None;
    }
    let value = &text[start..end];
    if value
        .split(' ')
        .any(|w| in_list(w, NOT_NAME) || in_list(w, TITLE_HEADS))
    {
        return None;
    }
    // Only an honorific in front is no name ("Dr. Said").
    let words: Vec<&str> = value.split(' ').filter(|w| !w.is_empty()).collect();
    if words.iter().filter(|w| !in_list(w, HONORIFICS)).count() == 0 {
        return None;
    }
    let mut end = end;
    if let Some(last) = words.last()
        && last.ends_with('.')
        && last.trim_end_matches('.').len() >= 3
        && words.len() > 1
    {
        end -= 1;
    }
    Some((start, end))
}

/// A person-shaped run that is, in full, the line `line` at `offset`.
fn person_line(text: &str, offset: usize, line: &str) -> Option<(usize, usize)> {
    let lead = line.len() - line.trim_start().len();
    let body = line.trim();
    if body.is_empty() {
        return None;
    }
    let (s, e) = person_at(text, offset + lead)?;
    (e - s == body.len()).then_some((s, e))
}

/// The person-shaped run that ends exactly at `end`, read back over the
/// capitalised words before it on its line, minus leading connectors.
fn person_ending_at(text: &str, end: usize) -> Option<(usize, usize)> {
    let line_start = text[..end].rfind('\n').map_or(0, |i| i + 1);
    let before = &text[line_start..end];
    let mut words: Vec<(usize, &str)> = Vec::new();
    let mut off = 0;
    for w in before.split(' ') {
        if !w.is_empty() {
            words.push((line_start + off, w));
        }
        off += w.len() + 1;
    }
    let mut first = words.len();
    while first > 0 && names_word(words[first - 1].1) && words.len() - first < 4 {
        first -= 1;
    }
    let tail = &words[first..];
    let skip = tail
        .iter()
        .take_while(|(_, w)| names::is_stop_word(w) || in_list(w, LEADS))
        .count();
    let &(start, _) = tail.get(skip)?;
    let (s, e) = person_at(text, start)?;
    (e == end).then_some((s, e))
}

/// A word that may be part of a name read backwards: capitalised, with no
/// punctuation that would end a run before it.
fn names_word(w: &str) -> bool {
    w.chars().next().is_some_and(|c| c.is_ascii_uppercase())
        && w.chars()
            .all(|c| c.is_ascii_alphabetic() || "'’.-".contains(c))
}

/// True for a job title line: up to seven capitalised words and connectors,
/// no digits, at least one title head ("Chief Executive Officer",
/// "Director of Procurement", "General Counsel").
fn is_title_line(line: &str) -> bool {
    let body = line.trim().trim_end_matches(',');
    let words: Vec<&str> = body.split_whitespace().collect();
    if words.is_empty() || words.len() > 7 || body.chars().any(|c| c.is_ascii_digit()) {
        return false;
    }
    let shaped = words.iter().all(|w| {
        matches!(*w, "of" | "and" | "&" | "the" | "for")
            || (w.chars().next().is_some_and(|c| c.is_ascii_uppercase())
                && w.trim_end_matches(',')
                    .chars()
                    .all(|c| c.is_ascii_alphabetic() || "-.&".contains(c)))
    });
    shaped && words.iter().any(|w| in_list(w, TITLE_HEADS))
}

/// A line that is, in full, an organisation ending in a suffix.
fn is_org_line(text: &str, offset: usize, line: &str) -> bool {
    let lead = line.len() - line.trim_start().len();
    let body = line.trim();
    !body.is_empty()
        && names::run_at(text, offset + lead)
            .is_some_and(|r| r.org && r.end - r.start == body.len())
}

/// A line of underscores or dashes (a place to sign).
fn is_rule_line(line: &str) -> bool {
    let body = line.trim();
    !body.is_empty() && body.chars().all(|c| matches!(c, '_' | '-' | ' '))
}

/// True when the first non-blank line `lines` yields starts with `prefix`
/// (lowercase), ignoring case and indent.
fn next_starts<'a>(mut lines: impl Iterator<Item = &'a (usize, &'a str)>, prefix: &str) -> bool {
    lines
        .find(|(_, l)| !l.trim().is_empty())
        .is_some_and(|(_, l)| l.trim_start().to_ascii_lowercase().starts_with(prefix))
}

struct Found {
    out: Vec<(usize, usize, &'static str)>,
}

impl Found {
    fn push(&mut self, span: Option<(usize, usize)>, role: &'static str) {
        if let Some((s, e)) = span
            && !self.out.iter().any(|&(a, b, _)| s < b && a < e)
        {
            self.out.push((s, e, role));
        }
    }
}

/// People in letters and agreements on one page, skipping spans already in
/// `taken`, as `[PERSON_n]` detections with their roles (an honorific in
/// prose gives none).
pub fn find(page: &PageText, taken: &[(usize, usize)]) -> Vec<Detection> {
    let text = page.text.as_str();
    let ls = lines(text);
    let mut f = Found { out: Vec::new() };

    // "Dear Odalys Fenwick," and "Dear Odalys,".
    let mut first_salutation = None;
    for c in SALUTATION.captures_iter(text) {
        let Some(inner) = c.get(1) else { continue };
        first_salutation.get_or_insert(inner.start());
        let value = inner.as_str();
        let words: Vec<&str> = value.split_whitespace().collect();
        if words
            .iter()
            .any(|w| in_list(w, NOT_NAME) || in_list(w, TITLE_HEADS) || names::is_stop_word(w))
        {
            continue;
        }
        let span = match words.as_slice() {
            [one] if one.len() >= 2 && names_word(one) && !one.contains('.') => {
                Some((inner.start(), inner.end()))
            }
            _ => person_at(text, inner.start()).filter(|&(_, e)| e == inner.end()),
        };
        f.push(span, "addressee");
    }

    // The name under a closing, past blank lines, a signing rule and the
    // company's own line.
    for m in CLOSING.find_iter(text) {
        let Some(i) = ls
            .iter()
            .position(|&(o, l)| o <= m.start() && m.start() <= o + l.len())
        else {
            continue;
        };
        for &(o, l) in ls.iter().skip(i + 1).take(5) {
            if l.trim().is_empty() || is_rule_line(l) || is_org_line(text, o, l) {
                continue;
            }
            if l.trim_start().starts_with("/s/") {
                continue;
            }
            f.push(person_line(text, o, l), "signatory");
            break;
        }
    }

    // A name line over a title line. Above the salutation it is the inside
    // address, so the addressee.
    for w in ls.windows(2) {
        let [(o, l), (_, next)] = w else { continue };
        if !is_title_line(next) {
            continue;
        }
        let role = if first_salutation.is_some_and(|s| *o < s) {
            "addressee"
        } else {
            "signatory"
        };
        f.push(person_line(text, *o, l), role);
    }

    // "Name:" lines in a signature block, "By:" and "/s/".
    for (i, &(o, l)) in ls.iter().enumerate() {
        let body = l.trim_start();
        let lead = l.len() - body.len();
        let lower = body.to_ascii_lowercase();
        if let Some(rest) = lower.strip_prefix("name:") {
            let by = next_starts(ls[..i].iter().rev(), "by:");
            let title = next_starts(ls[i + 1..].iter(), "title:");
            if by || title {
                let skip = rest.len() - rest.trim_start().len();
                let at = o + lead + "name:".len() + skip;
                if at < o + l.len() {
                    f.push(person_at(text, at), "signatory");
                }
            }
        }
        if let Some(rest) = lower.strip_prefix("by:") {
            let mut at = o + lead + "by:".len() + (rest.len() - rest.trim_start().len());
            if text[at..].starts_with("/s/") {
                at += "/s/".len();
                at += text[at..].len() - text[at..].trim_start_matches(' ').len();
            }
            if at < o + l.len() {
                f.push(person_at(text, at), "signatory");
            }
        }
    }
    for m in SLASH_S.find_iter(text) {
        if m.end() < text.len() {
            f.push(person_at(text, m.end()), "signatory");
        }
    }

    // Notices: "Attention:", "Attn:", "c/o", "with a copy to:".
    for m in NOTICE_LABEL.find_iter(text) {
        let Some(r) = names::name_after(text, m.end(), 10) else {
            continue;
        };
        if !r.org {
            f.push(person_at(text, r.start), "contact");
        }
    }
    // "Name, Esq." anywhere.
    for m in ESQ.find_iter(text) {
        f.push(person_ending_at(text, m.start()), "contact");
    }
    // "Name, Firm LLP" at the start of a line or after a colon.
    for &(o, l) in &ls {
        let body = l.trim_start();
        let mut starts = vec![o + l.len() - body.len()];
        if let Some(i) = l.find(':') {
            let after = &l[i + 1..];
            starts.push(o + i + 1 + after.len() - after.trim_start().len());
        }
        for s in starts {
            let Some((ps, pe)) = person_at(text, s) else {
                continue;
            };
            let rest = &text[pe..];
            if let Some(firm) = rest.strip_prefix(", ")
                && !firm.starts_with('\n')
                && names::run_at(text, pe + 2).is_some_and(|r| r.org)
            {
                f.push(Some((ps, pe)), "contact");
            }
        }
    }

    // cc lines and the lines under them up to a blank line (indented in
    // the original, flush left once OCR has flattened it).
    for m in CC_LINE.find_iter(text) {
        let Some(i) = ls
            .iter()
            .position(|&(o, l)| o <= m.start() && m.start() <= o + l.len())
        else {
            continue;
        };
        let mut segments = vec![(m.end(), ls[i].0 + ls[i].1.len())];
        for &(o, l) in ls[i + 1..].iter().take(6) {
            if l.trim().is_empty() {
                break;
            }
            segments.push((o, o + l.len()));
        }
        for (s, e) in segments {
            let mut at = s;
            for piece in text[s..e].split([',', ';']) {
                let lead = piece.len() - piece.trim_start().len();
                if !piece.trim().is_empty() {
                    f.push(person_at(text, at + lead), "contact");
                }
                at += piece.len() + 1;
            }
        }
    }

    // A party defined by name: `Ingrid Marchetti (the "Holder")`.
    for m in DEFINED.find_iter(text) {
        let before = text[..m.start()].trim_end_matches(' ');
        let before = before
            .strip_suffix(", an individual")
            .or_else(|| before.strip_suffix(" an individual"))
            .unwrap_or(before)
            .trim_end_matches(',');
        // `before` is a prefix of `text`, so its length is the run's end.
        f.push(person_ending_at(text, before.len()), "party");
    }

    // An honorific in prose: "Ms. Perpetua Quayle", "Mr. Quayle".
    let mut honorific: Vec<(usize, usize)> = Vec::new();
    for m in HONORIFIC.find_iter(text) {
        // After a capitalised word or a number, "Dr." is a street's Drive
        // ("Harbor Light Dr. Suite 210").
        let line_start = text[..m.start()].rfind('\n').map_or(0, |i| i + 1);
        let prev = text[line_start..m.start()].split_whitespace().last();
        if prev.is_some_and(|w| {
            w.starts_with(|c: char| c.is_ascii_uppercase() || c.is_ascii_digit())
                && !w.ends_with(|c: char| ".,;:()\"”".contains(c))
        }) {
            continue;
        }
        if let Some(span) = person_at(text, m.start()) {
            honorific.push(span);
        }
    }

    let mut out: Vec<Detection> = Vec::new();
    let mut seen: Vec<(usize, usize)> = taken.to_vec();
    let with_roles = f.out.into_iter().map(|(s, e, r)| (s, e, Some(r)));
    for (s, e, role) in with_roles.chain(honorific.into_iter().map(|(s, e)| (s, e, None))) {
        if overlaps(&seen, s, e) {
            continue;
        }
        seen.push((s, e));
        let mut d = detection(
            page.page,
            s,
            e,
            TokenKind::Person,
            rule("letter_name"),
            CONFIDENCE,
            Treatment::Tokenize,
        );
        d.role = role.map(String::from);
        out.push(d);
    }
    out
}

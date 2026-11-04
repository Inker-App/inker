//! One person, inked everywhere (added 2026-11-04). A letter names its
//! addressee after "Dear" and again in the page-2 header; an agreement names
//! a party in its opening paragraph and again under a signature line, and
//! cc's a colleague the body mentions. Once a person's name of two or more
//! words is placed by a template or a rule, every other occurrence of the
//! same words on any page, in any case, is inked as the same kind.
//!
//! Only whole names repeat: a given name or surname on its own is too
//! common a word to chase ("Grant", "Young").

use inker_core::{Detection, DetectionSource, PageText, TokenKind, Treatment};
use regex::Regex;

use crate::util::{detection, overlaps, rule};

/// Honorifics shed from a placed name before it is looked for elsewhere.
const HONORIFICS: &[&str] = &["mr.", "ms.", "mrs.", "mx.", "dr.", "prof."];

/// Fewest words in a name worth repeating.
const MIN_WORDS: usize = 2;

/// Every unplaced repeat of a placed person's name on `pages`, skipping
/// the pages in `skip` (a form's printed instructions).
pub fn find(pages: &[PageText], dets: &[Detection], skip: &[u32]) -> Vec<Detection> {
    let mut names: Vec<Vec<String>> = Vec::new();
    for d in dets {
        if d.kind != TokenKind::Person
            || d.field.is_some()
            || d.needs_review
            || d.treatment != Treatment::Tokenize
            || matches!(d.source, DetectionSource::FailClosed { .. })
        {
            continue;
        }
        let Some(v) = pages
            .iter()
            .find(|p| p.page == d.page)
            .and_then(|p| p.text.get(d.span.start..d.span.end))
        else {
            continue;
        };
        let words: Vec<String> = v
            .split_whitespace()
            .map(|w| w.trim_end_matches(',').to_string())
            .skip_while(|w| HONORIFICS.contains(&w.to_ascii_lowercase().as_str()))
            .collect();
        if words.len() >= MIN_WORDS && !names.contains(&words) {
            names.push(words);
        }
    }
    let patterns: Vec<Regex> = names
        .iter()
        .filter_map(|words| {
            let body: Vec<String> = words.iter().map(|w| regex::escape(w)).collect();
            Regex::new(&format!(r"(?i)\b{}\b", body.join(r"[ \t]+"))).ok()
        })
        .collect();
    let mut out = Vec::new();
    for p in pages.iter().filter(|p| !skip.contains(&p.page)) {
        let mut taken: Vec<(usize, usize)> = dets
            .iter()
            .filter(|d| d.page == p.page && d.field.is_none())
            .map(|d| (d.span.start, d.span.end))
            .collect();
        for re in &patterns {
            for m in re.find_iter(&p.text) {
                // Not glued to a longer word or hyphenated name.
                let b = p.text.as_bytes();
                let glued = |c: u8| c.is_ascii_alphanumeric() || c == b'-';
                if m.start() > 0 && glued(b[m.start() - 1])
                    || b.get(m.end()).copied().is_some_and(glued)
                    || overlaps(&taken, m.start(), m.end())
                {
                    continue;
                }
                taken.push((m.start(), m.end()));
                out.push(detection(
                    p.page,
                    m.start(),
                    m.end(),
                    TokenKind::Person,
                    rule("same_name"),
                    0.5,
                    Treatment::Tokenize,
                ));
            }
        }
    }
    out
}

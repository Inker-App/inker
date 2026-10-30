//! Letter-spaced text. A PDF text layer whose glyphs are placed one by one
//! (some tax software prints that way) or typed into comb boxes reads back
//! with a space between every character: "J o r d a n", "9 0 0 - 1 2 -
//! 3 4 5 6". No rule, template or fail-closed pass can read that, so a page
//! of it came through with nothing inked. The pipeline detects on a view with
//! those spaces closed and maps every span back to the original text.
//!
//! Two things close up:
//! - on any line, five or more single digits joined by single spaces (a
//!   lone `-` or `.` may sit between them): a comb-box SSN or PIN;
//! - on a line where most words are a single character, every single space
//!   next to a word of one or two characters (a glyph or a kerned pair),
//!   except between a letter and a digit and before a capital that follows
//!   a lower-case letter, so some word breaks survive. Wider gaps (columns)
//!   are kept.

use inker_core::{Detection, Extracted, PageText};

/// Words on a line before it can count as letter-spaced.
const MIN_WORDS: usize = 8;
/// Share of one-character words that makes a line letter-spaced, in percent.
const MIN_SINGLES_PCT: usize = 75;
/// Single digits in a row that close up on any line.
const MIN_DIGITS: usize = 5;

/// Page text with letter-spacing closed, and a byte map back to the original.
#[derive(Debug, Clone)]
pub struct Despaced {
    pub text: String,
    /// `to_orig[i]` is the original byte offset of byte `i` of `text`; one
    /// extra entry maps the end.
    to_orig: Vec<usize>,
}

impl Despaced {
    /// The original span for a span of the closed text. The end maps to the
    /// end of the original character under the last byte.
    pub fn span_to_orig(&self, start: usize, end: usize) -> (usize, usize) {
        let s = self.to_orig[start.min(self.text.len())];
        if end <= start {
            return (s, s);
        }
        let last = end.min(self.text.len()) - 1;
        let width = self.text[floor(&self.text, last)..]
            .chars()
            .next()
            .map_or(1, char::len_utf8);
        // The character under `last` starts at the floor; its original start
        // plus its width is the end.
        let o = self.to_orig[floor(&self.text, last)];
        (s, o + width)
    }
}

fn floor(s: &str, mut i: usize) -> usize {
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

/// One word of a line: byte range in the line and its first and last char.
#[derive(Debug, Clone, Copy)]
struct Word {
    start: usize,
    end: usize,
    first: char,
    last: char,
    /// The character before `last`, if the word has two or more.
    penult: Option<char>,
    /// One character.
    single: bool,
    /// At most two characters: a single glyph or a kerned pair ("Ex").
    short: bool,
}

fn words(line: &str) -> Vec<Word> {
    let mut out = Vec::new();
    let mut pos = 0;
    for w in line.split(' ') {
        if let (Some(first), Some(last)) = (w.chars().next(), w.chars().next_back()) {
            out.push(Word {
                start: pos,
                end: pos + w.len(),
                first,
                last,
                penult: w.chars().rev().nth(1),
                single: w.chars().nth(1).is_none(),
                short: w.chars().nth(2).is_none(),
            });
        }
        pos += w.len() + 1;
    }
    out
}

/// True when the space between `a` and `b` is a word break to keep on a
/// letter-spaced line. `before` is the character before `a` once closed up:
/// lower case after two capitals ends an acronym ("LLC to").
fn keep_break(before: Option<char>, a: char, b: char) -> bool {
    (a.is_alphabetic() && b.is_ascii_digit())
        || (a.is_ascii_digit() && b.is_alphabetic())
        || (a.is_lowercase() && b.is_uppercase())
        || (before.is_some_and(char::is_uppercase) && a.is_uppercase() && b.is_lowercase())
}

/// Byte offsets (in the line) of the single spaces to drop.
fn drops(line: &str) -> Vec<usize> {
    let ws = words(line);
    let mut out = Vec::new();
    let one_gap = |i: usize| ws[i + 1].start == ws[i].end + 1;

    // Runs of single digits, possibly with a lone '-' or '.' between them.
    let mut i = 0;
    while i < ws.len() {
        let is_digit = |w: &Word| w.single && w.first.is_ascii_digit();
        let is_sep = |w: &Word| w.single && matches!(w.first, '-' | '.');
        if !is_digit(&ws[i]) {
            i += 1;
            continue;
        }
        let mut j = i;
        let mut digits = 1;
        while j + 1 < ws.len() && one_gap(j) {
            let next = &ws[j + 1];
            if is_digit(next) {
                digits += 1;
                j += 1;
            } else if is_sep(next) && j + 2 < ws.len() && one_gap(j + 1) && is_digit(&ws[j + 2]) {
                j += 1;
            } else {
                break;
            }
        }
        if digits >= MIN_DIGITS {
            out.extend((i..j).map(|k| ws[k].end));
        }
        i = j + 1;
    }

    let singles = ws.iter().filter(|w| w.single).count();
    if ws.len() >= MIN_WORDS && singles * 100 >= ws.len() * MIN_SINGLES_PCT {
        let mut joined = false;
        for k in 0..ws.len().saturating_sub(1) {
            let (a, b) = (&ws[k], &ws[k + 1]);
            let before = match a.penult {
                Some(c) => Some(c),
                None if joined => Some(ws[k - 1].last),
                None => None,
            };
            joined = one_gap(k) && (a.short || b.short) && !keep_break(before, a.last, b.first);
            if joined {
                out.push(a.end);
            }
        }
    }
    out.sort_unstable();
    out.dedup();
    out
}

/// `text` with its letter-spacing closed, or `None` when nothing changes.
pub fn despace(text: &str) -> Option<Despaced> {
    let mut out = String::with_capacity(text.len());
    let mut to_orig = Vec::with_capacity(text.len() + 1);
    let mut changed = false;
    let mut line_start = 0;
    for line in text.split('\n') {
        let drop = drops(line);
        changed |= !drop.is_empty();
        let mut d = drop.iter().peekable();
        for (i, c) in line.char_indices() {
            if d.peek() == Some(&&i) {
                d.next();
                continue;
            }
            let start = out.len();
            out.push(c);
            to_orig.extend(std::iter::repeat_n(line_start + i, out.len() - start));
        }
        line_start += line.len();
        if line_start < text.len() {
            out.push('\n');
            to_orig.push(line_start);
            line_start += 1;
        }
    }
    to_orig.push(text.len());
    changed.then_some(Despaced { text: out, to_orig })
}

/// A document with every letter-spaced page closed up, and the maps back.
#[derive(Debug)]
pub(crate) struct View {
    pub extracted: Extracted,
    maps: Vec<(u32, Despaced)>,
}

impl View {
    /// `None` when no page of `extracted` is letter-spaced.
    pub fn of(extracted: &Extracted) -> Option<Self> {
        let mut maps = Vec::new();
        let pages: Vec<PageText> = extracted
            .pages
            .iter()
            .map(|p| match despace(&p.text) {
                Some(d) => {
                    let page = PageText {
                        page: p.page,
                        text: d.text.clone(),
                        origin: p.origin,
                    };
                    maps.push((p.page, d));
                    page
                }
                None => p.clone(),
            })
            .collect();
        if maps.is_empty() {
            return None;
        }
        Some(Self {
            extracted: Extracted {
                pages,
                fields: extracted.fields.clone(),
            },
            maps,
        })
    }

    /// Point page detections made on the view back at the original text.
    pub fn restore(&self, detections: &mut [Detection]) {
        for d in detections.iter_mut().filter(|d| d.field.is_none()) {
            if let Some((_, m)) = self.maps.iter().find(|(p, _)| *p == d.page) {
                let (s, e) = m.span_to_orig(d.span.start, d.span.end);
                d.span.start = s;
                d.span.end = e;
            }
        }
    }
}

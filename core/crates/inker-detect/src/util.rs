//! Small helpers shared by the detectors.

use inker_core::{Detection, DetectionSource, Span, TokenKind, Treatment};

/// Build a `Detection` with every field explicit. Review fields start empty.
pub fn detection(
    page: u32,
    start: usize,
    end: usize,
    kind: TokenKind,
    source: DetectionSource,
    confidence: f32,
    treatment: Treatment,
) -> Detection {
    Detection {
        page,
        span: Span { start, end },
        kind,
        source,
        confidence,
        treatment,
        needs_review: false,
        review_prompt: None,
        role: None,
        generalized: None,
        field: None,
    }
}

/// `DetectionSource::Rule` with a name.
pub(crate) fn rule(name: &str) -> DetectionSource {
    DetectionSource::Rule { name: name.into() }
}

/// Keep only the ASCII digits of `s`.
pub(crate) fn digits(s: &str) -> String {
    s.chars().filter(char::is_ascii_digit).collect()
}

/// Distance in bytes from the end of the nearest label (case-insensitive,
/// on word boundaries) that ends at or before `pos` and starts within
/// `window` bytes of it. `lower` must be `text.to_ascii_lowercase()` so
/// offsets line up.
pub(crate) fn label_before(
    lower: &str,
    pos: usize,
    window: usize,
    labels: &[&str],
) -> Option<usize> {
    let from = floor_char(lower, pos.saturating_sub(window));
    let pos = floor_char(lower, pos);
    let hay = &lower[from..pos];
    let bytes = lower.as_bytes();
    labels
        .iter()
        .flat_map(|l| {
            hay.match_indices(l).filter_map(move |(i, m)| {
                let start = from + i;
                let end = start + m.len();
                let before_ok = start == 0 || !bytes[start - 1].is_ascii_alphanumeric();
                let after_ok = end >= bytes.len() || !bytes[end].is_ascii_alphabetic();
                (before_ok && after_ok).then_some(pos - end)
            })
        })
        .min()
}

/// True when `text[start..end]` is not glued to a longer number or word: the
/// neighbours are not letters or digits, and not a separator (`-`, `.`, `,`,
/// `/`) followed or preceded by a digit.
pub(crate) fn isolated(text: &str, start: usize, end: usize) -> bool {
    let b = text.as_bytes();
    let sep = |c: u8| matches!(c, b'-' | b'.' | b',' | b'/');
    let before_ok = match start.checked_sub(1).map(|i| b[i]) {
        None => true,
        Some(c) if c.is_ascii_alphanumeric() => false,
        Some(c) if sep(c) => start < 2 || !b[start - 2].is_ascii_digit(),
        Some(_) => true,
    };
    let after_ok = match b.get(end) {
        None => true,
        Some(c) if c.is_ascii_alphanumeric() => false,
        Some(&c) if sep(c) => !b.get(end + 1).is_some_and(u8::is_ascii_digit),
        Some(_) => true,
    };
    before_ok && after_ok
}

/// Compile a regex written in this crate. The patterns are literals covered
/// by tests, so a failure is a programming error caught before release.
#[allow(clippy::expect_used)]
pub(crate) fn re(pattern: &str) -> regex::Regex {
    regex::Regex::new(pattern).expect("built-in regex compiles")
}

/// True when `[start, end)` overlaps any span in `taken`.
pub(crate) fn overlaps(taken: &[(usize, usize)], start: usize, end: usize) -> bool {
    taken.iter().any(|&(s, e)| start < e && s < end)
}

/// Largest char boundary at or below `i`.
pub(crate) fn floor_char(s: &str, mut i: usize) -> usize {
    if i >= s.len() {
        return s.len();
    }
    while !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

/// Smallest char boundary at or above `i`, capped at the length.
pub(crate) fn ceil_char(s: &str, mut i: usize) -> usize {
    if i >= s.len() {
        return s.len();
    }
    while !s.is_char_boundary(i) {
        i += 1;
    }
    i
}

/// Page text with look-alike characters folded so the digit rules see them:
/// dash variants (hyphen, non-breaking hyphen, figure dash, en and em dash,
/// U+2212 minus, fullwidth hyphen) become `-`, fullwidth digits become ASCII.
/// Keeps a byte map back to the original so spans can be translated.
#[derive(Debug)]
pub(crate) struct Folded {
    pub text: String,
    /// `to_orig[i]` is the original byte offset of folded byte `i`; one
    /// extra entry maps the end.
    to_orig: Vec<usize>,
}

impl Folded {
    pub fn new(text: &str) -> Self {
        let mut out = String::with_capacity(text.len());
        let mut to_orig = Vec::with_capacity(text.len() + 1);
        for (i, c) in text.char_indices() {
            let f = match c {
                '\u{2010}' | '\u{2011}' | '\u{2012}' | '\u{2013}' | '\u{2014}' | '\u{2212}'
                | '\u{fe63}' | '\u{ff0d}' => '-',
                '\u{ff10}'..='\u{ff19}' => {
                    char::from_u32(c as u32 - 0xff10 + u32::from(b'0')).unwrap_or(c)
                }
                _ => c,
            };
            for _ in 0..f.len_utf8() {
                to_orig.push(i);
            }
            out.push(f);
        }
        to_orig.push(text.len());
        Self { text: out, to_orig }
    }

    /// True when folding changed nothing.
    pub fn is_identity(&self, original: &str) -> bool {
        self.text == original
    }

    /// Original span for a folded span. A span end maps to the end of the
    /// original character before it.
    pub fn span_to_orig(&self, original: &str, start: usize, end: usize) -> (usize, usize) {
        let s = self.to_orig[start];
        let last = self.to_orig[end.saturating_sub(1).max(start)];
        let e = if end > start {
            last + original[last..].chars().next().map_or(0, char::len_utf8)
        } else {
            s
        };
        (s, e)
    }

    /// Folded span for an original span (first folded byte at or after each end).
    pub fn span_from_orig(&self, start: usize, end: usize) -> (usize, usize) {
        let f = |o: usize| self.to_orig.partition_point(|&x| x < o);
        (f(start), f(end))
    }
}

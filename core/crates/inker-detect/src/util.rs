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

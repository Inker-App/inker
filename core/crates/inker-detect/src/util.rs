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

/// Fewest digits a group needs to stand as a number of its own; shorter
/// groups are chunks of a longer value printed from digit boxes.
const MIN_ALONE: usize = 6;

static DIGIT_GROUP: std::sync::LazyLock<regex::Regex> =
    std::sync::LazyLock::new(|| re(r"\d[\d-]*"));

/// End of the value that starts at `[start, end)` once the digit groups
/// after it, each joined to the last by one space, are taken with it. Digit
/// boxes (routing and account numbers on Form 1040) come out of text layers
/// and OCR as "1 2 3 4 5 6 7 8 9" or "0001 2345 6789". A group joins when it
/// or the group before it is too short to be a number on its own, so two
/// whole numbers one space apart stay two values.
fn box_run_end(text: &str, start: usize, end: usize) -> usize {
    let b = text.as_bytes();
    let mut last = digits(&text[start..end]).len();
    let mut end = end;
    while b.get(end) == Some(&b' ') && b.get(end + 1).is_some_and(u8::is_ascii_digit) {
        let s = end + 1;
        let len = text[s..]
            .bytes()
            .take_while(|c| c.is_ascii_digit() || *c == b'-')
            .count();
        let e = s + text[s..s + len].trim_end_matches('-').len();
        let n = digits(&text[s..e]).len();
        if !isolated(text, s, e) || (last >= MIN_ALONE && n >= MIN_ALONE) {
            break;
        }
        end = e;
        last = n;
    }
    end
}

/// Every value in `text` made of digit groups, with groups printed from
/// digit boxes joined (see [`box_run_end`]). Groups glued to a word or part
/// of an amount are not values.
pub(crate) fn box_runs(text: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut pos = 0;
    for m in DIGIT_GROUP.find_iter(text) {
        let s = m.start();
        let e = s + m.as_str().trim_end_matches('-').len();
        if s < pos || !isolated(text, s, e) {
            continue;
        }
        let end = box_run_end(text, s, e);
        out.push((s, end));
        pos = end;
    }
    out
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

/// The parts of `[start, end)` outside every covered span, in order.
pub(crate) fn uncovered(
    start: usize,
    end: usize,
    covered: &[(usize, usize)],
) -> Vec<(usize, usize)> {
    let mut cuts: Vec<(usize, usize)> = covered
        .iter()
        .filter(|&&(s, e)| s < end && start < e)
        .copied()
        .collect();
    cuts.sort_unstable();
    let mut out = Vec::new();
    let mut pos = start;
    for (s, e) in cuts {
        if s > pos {
            out.push((pos, s));
        }
        pos = pos.max(e);
    }
    if pos < end {
        out.push((pos, end));
    }
    out
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

#[cfg(test)]
mod tests {
    use super::*;

    fn runs(text: &str) -> Vec<&str> {
        box_runs(text)
            .into_iter()
            .map(|(s, e)| &text[s..e])
            .collect()
    }

    #[test]
    fn one_digit_per_box_is_one_value() {
        assert_eq!(
            runs("Routing number    1 2 3 4 5 6 7 8 9   c Type"),
            ["1 2 3 4 5 6 7 8 9"]
        );
    }

    #[test]
    fn chunks_shorter_than_a_number_join_their_neighbours() {
        assert_eq!(runs("d 0001 2345 6789"), ["0001 2345 6789"]);
        assert_eq!(runs("d 00012 3456789"), ["00012 3456789"]);
    }

    #[test]
    fn two_whole_numbers_stay_apart() {
        assert_eq!(
            runs("123456789 000123456789"),
            ["123456789", "000123456789"]
        );
        assert_eq!(
            runs("1 2 3 4 5 6 7 8 9    0 0 0 1 2 3 4 5 6 7 8 9"),
            ["1 2 3 4 5 6 7 8 9", "0 0 0 1 2 3 4 5 6 7 8 9"]
        );
    }

    #[test]
    fn amounts_and_words_end_a_run() {
        assert_eq!(runs("12345678 5,000.00"), ["12345678"]);
        assert_eq!(runs("1 2 3 4a"), ["1 2 3"]);
    }

    #[test]
    fn run_end_extends_a_value_over_its_boxes() {
        let t = "d 0001 2345 6789   36";
        assert_eq!(box_run_end(t, 2, 6), 16);
        assert_eq!(box_run_end("123456789 000123456789", 0, 9), 9);
    }
}

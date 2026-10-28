//! The attention card's peek: a few words either side of a held value, safe
//! to show. Never the value itself, and never another number: any run of 4+
//! digits in the context becomes `…`.

use crate::Span;

/// Characters of context kept on each side of a held value.
pub const CONTEXT_CHARS: usize = 24;

/// `(before, after)` around `span` in `text`. Empty strings when the span
/// does not fit the text.
pub fn context(text: &str, span: Span) -> (String, String) {
    let (Some(before), Some(after)) = (
        text.get(..span.start),
        text.get(span.end..)
            .filter(|_| text.get(span.start..span.end).is_some()),
    ) else {
        return (String::new(), String::new());
    };
    (side(before, Side::Before), side(after, Side::After))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Side {
    Before,
    After,
}

/// The [`CONTEXT_CHARS`] characters of `text` nearest the value, cut back to
/// a whole word, whitespace flattened, long digit runs masked. A cut is
/// marked with `…` on the side away from the value.
fn side(text: &str, which: Side) -> String {
    let chars: Vec<char> = text.chars().collect();
    let cut = chars.len() > CONTEXT_CHARS;
    let (window, beyond) = match which {
        Side::Before if cut => {
            let at = chars.len() - CONTEXT_CHARS;
            (&chars[at..], chars.get(at - 1).copied())
        }
        Side::After if cut => (&chars[..CONTEXT_CHARS], chars.get(CONTEXT_CHARS).copied()),
        _ => (&chars[..], None),
    };
    let mut window: Vec<char> = window.to_vec();
    // A word split by the window's far edge is dropped whole.
    if beyond.is_some_and(|c| !c.is_whitespace()) {
        match which {
            Side::Before => {
                let keep = window.iter().position(|c| c.is_whitespace());
                window.drain(..keep.unwrap_or(window.len()));
            }
            Side::After => {
                let keep = window.iter().rposition(|c| c.is_whitespace());
                window.truncate(keep.map_or(0, |i| i + 1));
            }
        }
    }
    let flat = mask_digits(&flatten(&window));
    if flat.trim().is_empty() {
        return String::new();
    }
    match (which, cut) {
        (Side::Before, true) => format!("…{}", flat.trim_start()),
        (Side::After, true) => format!("{}…", flat.trim_end()),
        _ => flat,
    }
}

/// Every whitespace run becomes one space.
fn flatten(chars: &[char]) -> String {
    let mut out = String::with_capacity(chars.len());
    let mut in_space = false;
    for &c in chars {
        if c.is_whitespace() {
            if !in_space {
                out.push(' ');
            }
            in_space = true;
        } else {
            out.push(c);
            in_space = false;
        }
    }
    out
}

/// Every run of 4 or more digits becomes `…`.
fn mask_digits(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut run = String::new();
    let flush = |run: &mut String, out: &mut String| {
        if run.chars().count() >= 4 {
            out.push('…');
        } else {
            out.push_str(run);
        }
        run.clear();
    };
    for c in text.chars() {
        if c.is_numeric() {
            run.push(c);
        } else {
            flush(&mut run, &mut out);
            out.push(c);
        }
    }
    flush(&mut run, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn around(text: &str, needle: &str) -> (String, String) {
        let start = text.find(needle).unwrap();
        context(
            text,
            Span {
                start,
                end: start + needle.len(),
            },
        )
    }

    #[test]
    fn keeps_the_words_either_side() {
        let (b, a) = around("wire to 00000000000 ref 4471", "00000000000");
        assert_eq!(b, "wire to ");
        assert_eq!(a, " ref …");
    }

    #[test]
    fn trims_to_a_word_boundary_and_marks_the_cut() {
        let text = "the quick brown fox jumped over the lazy dog 00000000000 and then they ran far away home";
        let (b, a) = around(text, "00000000000");
        assert!(b.chars().count() <= CONTEXT_CHARS + 1, "{b:?}");
        assert_eq!(b, "…over the lazy dog ");
        assert_eq!(a, " and then they ran far…");
    }

    #[test]
    fn a_window_that_ends_on_a_word_boundary_keeps_its_first_word() {
        // Exactly 24 characters before the value, preceded by a space.
        let text = "x abcdefghij klmnopqrstuv 00000000000";
        let (b, _) = around(text, "00000000000");
        assert_eq!(b, "…abcdefghij klmnopqrstuv ");
    }

    #[test]
    fn never_carries_another_number() {
        let (b, a) = around("ref 123456 acct 000000000 ext 12345678", "000000000");
        assert_eq!(b, "ref … acct ");
        assert_eq!(a, " ext …");
        let (b, _) = around("box 12 line 345 000000000", "000000000");
        assert_eq!(b, "box 12 line 345 ");
    }

    #[test]
    fn whitespace_is_flattened() {
        let (b, a) = around("Account\n  number:\t000000000\nnext line", "000000000");
        assert_eq!(b, "Account number: ");
        assert_eq!(a, " next line");
    }

    #[test]
    fn a_single_long_word_gives_nothing() {
        let (b, _) = around("abcdefghijklmnopqrstuvwxyzabcdef 000000000", "000000000");
        assert_eq!(b, "");
    }

    #[test]
    fn bad_spans_give_nothing() {
        assert_eq!(
            context("short", Span { start: 3, end: 99 }),
            (String::new(), String::new())
        );
        assert_eq!(
            context("é", Span { start: 1, end: 2 }),
            (String::new(), String::new())
        );
    }
}

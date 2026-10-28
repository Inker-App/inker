//! Value shapes that are safe to leave in the clear: amounts and short
//! enumerated values (a state or box code, a date, a checkbox mark).
//!
//! Template `keep` boxes cover only these, and a form-field value made of
//! nothing else is not held.

use std::sync::LazyLock;

use regex::Regex;

use crate::util::re;

/// Money: optional sign or parentheses, optional `$`, digits with optional
/// thousands separators and decimals. Without separators or decimals at most
/// five digits, so a run of six or more digits is never an amount.
static AMOUNT: LazyLock<Regex> = LazyLock::new(|| {
    re(
        r"^[-(]?\$?-?(?:\d{1,3}(?:,\d{3})+(?:\.\d{1,2})?|\d{1,5}(?:\.\d{1,2})?|\d{6,9}\.\d{2})\)?%?$",
    )
});
static DATE: LazyLock<Regex> =
    LazyLock::new(|| re(r"^(?:\d{1,2}[/-]\d{1,2}[/-](?:\d{2}|\d{4})|\d{4}-\d{2}-\d{2})$"));
/// A state or box code: one or two capital letters, or a digit and a capital
/// (1095-C offer codes such as `1E`).
static CODE: LazyLock<Regex> = LazyLock::new(|| re(r"^(?:[A-Z]{1,2}|\d[A-Z])$"));
const MARKS: &[&str] = &["x", "✓", "✔", "yes", "no", "on", "off", "true", "false"];

/// True when `token` (no whitespace) is an amount.
pub fn is_amount(token: &str) -> bool {
    AMOUNT.is_match(token)
}

/// True when `token` is an amount, a date, a state or box code or a checkbox mark.
pub fn is_benign_token(token: &str) -> bool {
    is_amount(token)
        || DATE.is_match(token)
        || CODE.is_match(token)
        || MARKS.contains(&token.to_lowercase().as_str())
}

/// Byte ranges of the whitespace-separated tokens of `text`.
pub fn tokens(text: &str) -> impl Iterator<Item = (usize, usize)> + '_ {
    let mut pos = 0;
    std::iter::from_fn(move || {
        let rest = &text[pos..];
        let skip = rest.len() - rest.trim_start().len();
        let start = pos + skip;
        if start >= text.len() {
            return None;
        }
        let len = text[start..]
            .find(char::is_whitespace)
            .unwrap_or(text.len() - start);
        pos = start + len;
        Some((start, start + len))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shapes() {
        for t in [
            "52000.00",
            "1,204.55",
            "$96,400.00",
            "(150.00)",
            "-150.00",
            "0",
            "12345",
        ] {
            assert!(is_amount(t), "{t}");
        }
        for t in ["123456", "4471002233119", "1,2345", "4471002233.11", "ref"] {
            assert!(!is_amount(t), "{t}");
        }
        for t in ["CO", "D", "1E", "X", "Off", "03/14/2025", "2025-03-14"] {
            assert!(is_benign_token(t), "{t}");
        }
        for t in ["Riley", "COX", "ref", "CW-0042-77"] {
            assert!(!is_benign_token(t), "{t}");
        }
        let s = " a  bc\nd ";
        let got: Vec<&str> = tokens(s).map(|(a, b)| &s[a..b]).collect();
        assert_eq!(got, ["a", "bc", "d"]);
    }
}

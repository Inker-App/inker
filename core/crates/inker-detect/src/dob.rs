//! Dates of birth. A date is inked only when a birth-date label ("Date of
//! birth", "DOB", "Birth date") comes shortly before it with no other number
//! in between; the tax year, a signature date or a form's revision date stay
//! in the clear. The kind is `Custom("dob")`, rendered `[DOB_1]`.

use std::sync::LazyLock;

use inker_core::{Detection, PageText, TokenKind, Treatment};
use regex::Regex;

use crate::util::{Folded, detection, label_before, re, rule};

/// Labels that make the date after them a date of birth.
pub const DOB_LABELS: &[&str] = &[
    "date of birth",
    "birth date",
    "birthdate",
    "dob",
    "d.o.b.",
    "d.o.b",
];
/// How far before a date a birth-date label may end.
pub const WINDOW: usize = 40;
const CONF: f32 = 0.8;

const MONTHS: &str = "jan(?:uary)?|feb(?:ruary)?|mar(?:ch)?|apr(?:il)?|may|june?|july?|aug(?:ust)?|sep(?:t(?:ember)?)?|oct(?:ober)?|nov(?:ember)?|dec(?:ember)?";

/// `MM/DD/YYYY`, `M-D-YY`, `MM.DD.YYYY`, `YYYY-MM-DD`, `Month D, YYYY`,
/// `Mon. D YYYY` and `D Month YYYY`.
static DATE: LazyLock<Regex> = LazyLock::new(|| {
    re(&format!(
        r"(?i)\b(?:\d{{1,2}}[/.-]\d{{1,2}}[/.-](?:\d{{4}}|\d{{2}})|\d{{4}}-\d{{1,2}}-\d{{1,2}}|(?:{MONTHS})\.? \d{{1,2}},? \d{{4}}|\d{{1,2}} (?:{MONTHS})\.?,? \d{{4}})\b"
    ))
});

/// Dates of birth on one page.
pub fn find(page: &PageText) -> Vec<Detection> {
    // Folding keeps ASCII byte for byte; spans are mapped back regardless.
    let folded = Folded::new(&page.text);
    let text = folded.text.as_str();
    let lower = text.to_ascii_lowercase();
    let mut out = Vec::new();
    for m in DATE.find_iter(text) {
        let Some(dist) = label_before(&lower, m.start(), WINDOW, DOB_LABELS) else {
            continue;
        };
        // No other number between the label and this date.
        let gap = &text[m.start() - dist..m.start()];
        if gap.chars().any(|c| c.is_ascii_digit()) {
            continue;
        }
        let (s, e) = folded.span_to_orig(&page.text, m.start(), m.end());
        out.push(detection(
            page.page,
            s,
            e,
            TokenKind::Custom("dob".into()),
            rule("dob"),
            CONF,
            Treatment::Tokenize,
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use inker_core::TextOrigin;

    fn found(text: &str) -> Vec<&str> {
        let p = PageText {
            page: 1,
            text: text.into(),
            origin: TextOrigin::TextLayer,
        };
        find(&p)
            .into_iter()
            .map(|d| &text[d.span.start..d.span.end])
            .collect()
    }

    #[test]
    fn dates_after_a_birth_label() {
        assert_eq!(found("Date of birth 03/14/1985"), ["03/14/1985"]);
        assert_eq!(found("DOB: 3-4-85"), ["3-4-85"]);
        assert_eq!(found("Birth date March 14, 1985"), ["March 14, 1985"]);
        assert_eq!(
            found("Date of birth (MM/DD/YYYY)   1985-03-14"),
            ["1985-03-14"]
        );
        assert_eq!(found("Birthdate 14 Mar 1985"), ["14 Mar 1985"]);
        assert_eq!(found("D.O.B. Sept. 4, 1990"), ["Sept. 4, 1990"]);
    }

    #[test]
    fn other_dates_stay_clear() {
        assert!(found("Date 04/10/2024   Your occupation").is_empty());
        assert!(found("Were born before January 2, 1959").is_empty());
        assert!(found("For the year Jan. 1-Dec. 31, 2023").is_empty());
        // A second date after the birth date is not one.
        assert_eq!(found("DOB 03/14/1985 signed 04/10/2024"), ["03/14/1985"]);
        // Too far from the label.
        assert!(found(&format!("Date of birth{}03/14/1985", " ".repeat(50))).is_empty());
    }
}

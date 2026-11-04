//! The inked view: a document's pages exactly as `resolve` gives them to an
//! agent, with every token located so the shell can draw it as a chip.
//!
//! Tokens are found with `inker_core::vault::find_tokens`, the same scanner
//! the core uses to sweep and rehydrate them, so the view and the core agree
//! on what a token is.

use inker_core::vault::find_tokens;

use crate::error::FfiError;
use crate::types::{InkedPage, InkedToken, TextSpan};

/// The document is held for the attention card.
pub const PENDING_REVIEW: &str = "pending review";
/// Queued, being read or inked, or failed.
pub const NOT_READY: &str = "not ready";
/// A folder id: a folder's `resolve` is a listing, not text.
pub const FOLDER: &str = "a folder has no inked view";

/// The fixed, value-free errors the shell shows as empty states.
pub fn pending_review() -> FfiError {
    FfiError::Backend {
        reason: PENDING_REVIEW.into(),
    }
}

pub fn not_ready() -> FfiError {
    FfiError::Backend {
        reason: NOT_READY.into(),
    }
}

pub fn folder() -> FfiError {
    FfiError::InvalidArgument {
        reason: FOLDER.into(),
    }
}

pub fn not_found() -> FfiError {
    FfiError::NotFound {
        reason: "no document with that id".into(),
    }
}

/// One page with its tokens. `roles` maps a rendered token (`[SSN_1]`) to
/// its role; a token with several roles in this document gets them joined.
pub fn page(page: u32, text: String, roles: &[(String, String)]) -> InkedPage {
    let tokens = find_tokens(&text)
        .into_iter()
        .map(|(range, label, _)| {
            let token = text[range.clone()].to_string();
            let mut found: Vec<&str> = Vec::new();
            for (t, role) in roles {
                if *t == token && !found.contains(&role.as_str()) {
                    found.push(role);
                }
            }
            InkedToken {
                span: span(range.start, range.end),
                role: (!found.is_empty()).then(|| found.join(", ")),
                token,
                kind_label: label,
            }
        })
        .collect();
    InkedPage { page, text, tokens }
}

fn span(start: usize, end: usize) -> TextSpan {
    TextSpan {
        start: u32::try_from(start).unwrap_or(u32::MAX),
        end: u32::try_from(end).unwrap_or(u32::MAX),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn roles() -> Vec<(String, String)> {
        vec![
            ("[SSN_1]".into(), "employee".into()),
            ("[ORG_1]".into(), "employer".into()),
            ("[ORG_1]".into(), "payer".into()),
            ("[ORG_1]".into(), "employer".into()),
        ]
    }

    #[test]
    fn tokens_carry_byte_spans_kinds_and_roles() {
        let text = "SSN [SSN_1] for [PERSON_2]".to_string();
        let p = page(1, text.clone(), &roles());
        assert_eq!(p.page, 1);
        assert_eq!(p.text, text);
        assert_eq!(p.tokens.len(), 2);
        assert_eq!(p.tokens[0].token, "[SSN_1]");
        assert_eq!(p.tokens[0].span, span(4, 11));
        assert_eq!(p.tokens[0].kind_label, "SSN");
        assert_eq!(p.tokens[0].role.as_deref(), Some("employee"));
        assert_eq!(p.tokens[1].kind_label, "PERSON");
        assert_eq!(p.tokens[1].role, None, "no role in describe");
    }

    #[test]
    fn spans_are_bytes_in_multi_byte_text() {
        let text = "Café · 🐙 [ORG_1]".to_string();
        let p = page(2, text.clone(), &roles());
        let t = &p.tokens[0];
        assert_eq!(&text[t.span.start as usize..t.span.end as usize], "[ORG_1]");
        assert_eq!(t.span.start as usize, text.find('[').unwrap());
        assert_eq!(t.role.as_deref(), Some("employer, payer"), "each role once");
    }

    #[test]
    fn adjacent_tokens_and_labels_with_underscores() {
        let p = page(1, "[SSN_1][PLAN_ID_12]".into(), &[]);
        let got: Vec<(&str, &str, TextSpan)> = p
            .tokens
            .iter()
            .map(|t| (t.token.as_str(), t.kind_label.as_str(), t.span.clone()))
            .collect();
        assert_eq!(
            got,
            vec![
                ("[SSN_1]", "SSN", span(0, 7)),
                ("[PLAN_ID_12]", "PLAN_ID", span(7, 19)),
            ]
        );
    }

    #[test]
    fn brackets_that_are_not_tokens_stay_text() {
        let p = page(1, "[x] [SSN_] [ssn_1] [SSN_1a] [ ] [".into(), &[]);
        assert!(p.tokens.is_empty(), "{:?}", p.tokens);
        assert!(page(1, String::new(), &[]).tokens.is_empty());
    }

    #[test]
    fn the_fixed_errors_are_value_free_strings() {
        assert!(
            matches!(pending_review(), FfiError::Backend { reason } if reason == "pending review")
        );
        assert!(matches!(not_ready(), FfiError::Backend { reason } if reason == "not ready"));
        assert!(
            matches!(folder(), FfiError::InvalidArgument { reason } if reason == "a folder has no inked view")
        );
        assert!(matches!(not_found(), FfiError::NotFound { .. }));
    }
}

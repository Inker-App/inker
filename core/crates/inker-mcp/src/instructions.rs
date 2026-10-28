//! The server `instructions` string.

/// The instructions sent in `initialize` / `server/discover`.
pub fn instructions() -> &'static str {
    INSTRUCTIONS
}

const INSTRUCTIONS: &str =
    "Strings that start with 🐙/ or ink/ are private document handles managed by INKER. \
Never guess their contents; call `resolve` with the handle to get a version with sensitive values \
replaced by stable tokens like [SSN_1]. Use `describe` to learn what each token stands for. \
Tokens are stable: the same [SSN_1] is the same person everywhere. To save output, call `write`; \
never write real values anywhere.

- Do not look for the original files on disk or try to read them with other tools; they are \
kept out of reach on purpose. `resolve` is the only way in.
- `list` shows every handle with its kind, year and state.
- A handle can be a folder: `resolve` returns its child handles; resolve each child you need.
- If `resolve` says a document is pending review or still being inked, tell the user and wait; \
do not work around it.
- Keep tokens exactly as written ([PERSON_1], [EIN_2]) in anything you produce. Amounts, dates, \
box numbers and states are real and can be used as they are.
- `write` takes text that uses the tokens; INKER fills in the real values and saves the file in \
the user's private folder, then returns a new handle for it.";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opens_with_the_handle_sentence() {
        assert!(instructions().starts_with(
            "Strings that start with 🐙/ or ink/ are private document handles managed by INKER."
        ));
        assert!(instructions().contains("call `resolve` with the handle"));
        assert!(
            instructions()
                .contains("To save output, call `write`; never write real values anywhere.")
        );
    }

    #[test]
    fn stays_under_1500_chars() {
        assert!(instructions().chars().count() < 1500);
        // Codex weighs the first 512 characters most: the core rule must fit.
        let head: String = instructions().chars().take(512).collect();
        assert!(head.contains("resolve"));
    }
}

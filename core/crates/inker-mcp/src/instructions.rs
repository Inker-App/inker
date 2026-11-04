//! The server `instructions` string, and the Claude skill that repeats it.

/// One literal for both, so the skill can never drift from the instructions.
macro_rules! instructions_text {
    () => {
        "Strings that start with 🐙/ or ink/ are private document handles managed by INKER: read one \
with INKER's `resolve` tool, never from disk. Never guess their contents; call `resolve` with the handle to get a version with sensitive values \
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
the user's private folder, then returns a new handle for it."
    };
}

/// The instructions sent in `initialize` / `server/discover`.
pub fn instructions() -> &'static str {
    INSTRUCTIONS
}

/// A `SKILL.md` for Claude Desktop and claude.ai. Those apps do not apply
/// server instructions, but a skill's description is always in the model's
/// context, so a user who installs this one gives every chat the handle rule.
pub fn skill() -> &'static str {
    SKILL
}

const INSTRUCTIONS: &str = instructions_text!();

const SKILL: &str = concat!(
    "---\n",
    "name: inker\n",
    "description: Use this whenever a message contains 🐙/ or ink/ followed by a name, because \
     that is an INKER private document handle to read with the INKER `resolve` tool, never from \
     disk.\n",
    "---\n",
    "\n# INKER handles\n\n",
    instructions_text!()
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opens_with_the_handle_sentence() {
        assert!(instructions().starts_with(
            "Strings that start with 🐙/ or ink/ are private document handles managed by INKER: \
             read one with INKER's `resolve` tool, never from disk."
        ));
        assert!(instructions().contains("call `resolve` with the handle"));
        assert!(
            instructions()
                .contains("To save output, call `write`; never write real values anywhere.")
        );
    }

    fn skill_frontmatter() -> (String, String) {
        let rest = skill()
            .strip_prefix("---\n")
            .expect("opens with frontmatter");
        let (front, body) = rest.split_once("\n---\n").expect("closes frontmatter");
        (front.to_string(), body.to_string())
    }

    #[test]
    fn skill_is_named_inker_with_a_one_sentence_trigger() {
        let (front, _) = skill_frontmatter();
        let lines: Vec<&str> = front.lines().collect();
        assert_eq!(lines.len(), 2, "{front}");
        assert_eq!(lines[0], "name: inker");
        let desc = lines[1].strip_prefix("description: ").expect("description");
        // A plain YAML scalar: no ": " or leading quote to trip a parser.
        assert!(
            !desc.contains(": ") && !desc.starts_with(['"', '\'']),
            "{desc}"
        );
        assert!(desc.len() <= 1024, "skill descriptions are capped");
        assert_eq!(desc.matches(". ").count(), 0, "one sentence: {desc}");
        assert!(desc.ends_with('.'));
        for cue in [
            "🐙/",
            "ink/",
            "INKER",
            "private document",
            "`resolve`",
            "never from disk",
        ] {
            assert!(desc.contains(cue), "{cue:?} not in {desc}");
        }
    }

    #[test]
    fn skill_body_repeats_the_instructions() {
        let (_, body) = skill_frontmatter();
        assert!(body.contains(instructions()), "{body}");
        assert!(body.starts_with("\n# INKER handles\n"), "{body}");
    }

    #[test]
    fn stays_under_1500_chars() {
        assert!(instructions().chars().count() < 1500);
        // Codex weighs the first 512 characters most: the core rule must fit.
        let head: String = instructions().chars().take(512).collect();
        assert!(head.contains("resolve"));
    }
}

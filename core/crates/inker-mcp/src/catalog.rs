//! Resources, the `read` prompt and handle completions.
//!
//! Chat apps show resources and prompts in their "+" menu, so the user can
//! hand a document to the model without the model having to find the
//! `resolve` tool first. Every string here goes to an agent: handles, kinds,
//! years and counts only.

use inker_core::{Codename, DocState, Handle, ListEntry};
use rmcp::model::{Prompt, PromptArgument, Resource, ResourceTemplate};

/// The one URI scheme INKER serves. ASCII only: no emoji in a URI.
pub const RESOURCE_SCHEME: &str = "inker:///";

/// `resources/templates/list`'s single template.
pub const RESOURCE_TEMPLATE: &str = "inker:///{codename}";

/// The template's one variable, for `completion/complete`.
pub const TEMPLATE_ARGUMENT: &str = "codename";

/// The prompt's name and its one argument.
pub const READ_PROMPT: &str = "read";
pub const READ_PROMPT_ARGUMENT: &str = "handle";

/// Resources are plain text: the same text `resolve` returns.
pub const MIME_TYPE: &str = "text/plain";

/// The spec caps a completion response at 100 values.
const MAX_COMPLETIONS: usize = 100;

const OCTOPUS: &str = "🐙/";
const ASCII: &str = "ink/";

/// `inker:///w2-acme-2025` for `🐙/w2-acme-2025`.
pub fn resource_uri(h: &Handle) -> String {
    format!("{RESOURCE_SCHEME}{}", h.codename)
}

/// The handle a resource URI names, or `None` for anything that is not
/// exactly `inker:///<codename>`.
pub fn handle_from_uri(uri: &str) -> Option<Handle> {
    let codename = uri.strip_prefix(RESOURCE_SCHEME)?;
    Codename::parse(codename).ok().map(Handle::new)
}

/// `w2 · 2025 · 6 inked`: the list entry without its handle and state.
pub fn summary(e: &ListEntry) -> String {
    let mut s = e.kind.as_str().to_string();
    if let Some(y) = e.year {
        s.push_str(&format!(" · {y}"));
    }
    s.push_str(&format!(" · {} inked", e.inked));
    if e.is_folder && e.kind.as_str() != "folder" {
        s.push_str(" · folder");
    }
    s
}

/// One resource per ready document and per folder. Held and in-flight
/// documents are left out: reading them would only give a refusal.
pub fn resources(entries: &[ListEntry]) -> Vec<Resource> {
    entries
        .iter()
        .filter(|e| e.is_folder || e.state == DocState::Ready)
        .map(|e| {
            let shown = e.handle.to_string();
            Resource::new(resource_uri(&e.handle), shown.clone())
                .with_title(shown)
                .with_description(summary(e))
                .with_mime_type(MIME_TYPE)
        })
        .collect()
}

/// The template for any INKER handle.
pub fn template() -> ResourceTemplate {
    ResourceTemplate::new(RESOURCE_TEMPLATE, "INKER handle")
        .with_title("INKER handle")
        .with_description(
            "An INKER private document or folder by its codename: the handle \
             🐙/w2-acme-2025 (or ink/w2-acme-2025) is inker:///w2-acme-2025.",
        )
        .with_mime_type(MIME_TYPE)
}

/// The `read` prompt as `prompts/list` shows it.
pub fn read_prompt() -> Prompt {
    Prompt::new(
        READ_PROMPT,
        Some(
            "Have the model read an INKER private document handle (🐙/… or ink/…) with \
             INKER's `resolve` tool instead of looking for a file.",
        ),
        Some(vec![
            PromptArgument::new(READ_PROMPT_ARGUMENT)
                .with_description("The INKER handle, like 🐙/w2-acme-2025 or ink/w2-acme-2025.")
                .with_required(true),
        ]),
    )
    .with_title("Read an INKER handle")
}

/// The single user message `prompts/get` returns for `read`.
pub fn read_prompt_text(h: &Handle) -> String {
    format!(
        "Read {h} with INKER's `resolve` tool, then tell me what it contains. It is a private \
         document handle managed by INKER, not a file path; do not look for it on disk. Keep \
         tokens like [SSN_1] exactly as written."
    )
}

/// Handles completing `value` for the prompt's `handle` argument. The prefix
/// the user typed (`🐙/` or `ink/`) is the prefix offered; a value with no
/// prefix matches codenames and is offered as `🐙/`.
pub fn complete_handle(entries: &[ListEntry], value: &str) -> Vec<String> {
    let value = value.trim_start();
    let (prefix, rest) = if let Some(rest) = value.strip_prefix(OCTOPUS) {
        (OCTOPUS, rest)
    } else if let Some(rest) = value.strip_prefix(ASCII) {
        (ASCII, rest)
    } else {
        (OCTOPUS, value)
    };
    entries
        .iter()
        .map(|e| e.handle.codename.as_str())
        .filter(|c| c.starts_with(rest))
        .map(|c| format!("{prefix}{c}"))
        .take(MAX_COMPLETIONS)
        .collect()
}

/// Bare codenames completing `value`, for the template's `codename`.
pub fn complete_codename(entries: &[ListEntry], value: &str) -> Vec<String> {
    entries
        .iter()
        .map(|e| e.handle.codename.as_str())
        .filter(|c| c.starts_with(value.trim_start()))
        .map(str::to_string)
        .take(MAX_COMPLETIONS)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use inker_core::DocKind;

    fn entry(codename: &str, state: DocState, is_folder: bool) -> ListEntry {
        ListEntry {
            handle: Handle::new(Codename::parse(codename).unwrap()),
            kind: DocKind(if is_folder { "folder" } else { "w2" }.into()),
            year: Some(2025),
            state,
            inked: 3,
            is_folder,
        }
    }

    #[test]
    fn uris_round_trip_and_reject_anything_else() {
        let h = Handle::new(Codename::parse("w2-acme-2025").unwrap());
        assert_eq!(resource_uri(&h), "inker:///w2-acme-2025");
        assert_eq!(handle_from_uri("inker:///w2-acme-2025"), Some(h));
        for bad in [
            "inker:///",
            "inker:///../x",
            "inker://w2-acme-2025",
            "file:///w2-acme-2025",
            "inker:///W2",
            "inker:///a/b",
        ] {
            assert_eq!(handle_from_uri(bad), None, "{bad}");
        }
    }

    #[test]
    fn only_ready_documents_and_folders_are_resources() {
        let entries = [
            entry("a", DocState::Ready, false),
            entry("b", DocState::NeedsAttention, false),
            entry("c", DocState::Inking, false),
            entry("d", DocState::Queued, true),
        ];
        let uris: Vec<String> = resources(&entries).into_iter().map(|r| r.uri).collect();
        assert_eq!(uris, ["inker:///a", "inker:///d"]);
    }

    #[test]
    fn summary_reads_like_the_board() {
        assert_eq!(
            summary(&entry("a", DocState::Ready, false)),
            "w2 · 2025 · 3 inked"
        );
        let mut f = entry("d", DocState::Ready, true);
        assert_eq!(summary(&f), "folder · 2025 · 3 inked");
        f.kind = DocKind("taxes".into());
        f.year = None;
        assert_eq!(summary(&f), "taxes · 3 inked · folder");
    }

    #[test]
    fn completions_keep_the_typed_prefix() {
        let entries = [
            entry("w2-acme-2025", DocState::Ready, false),
            entry("taxes-2025", DocState::Ready, true),
        ];
        assert_eq!(complete_handle(&entries, "ink/t"), ["ink/taxes-2025"]);
        assert_eq!(complete_handle(&entries, "🐙/w"), ["🐙/w2-acme-2025"]);
        assert_eq!(complete_handle(&entries, "w"), ["🐙/w2-acme-2025"]);
        assert_eq!(complete_codename(&entries, "t"), ["taxes-2025"]);
    }
}

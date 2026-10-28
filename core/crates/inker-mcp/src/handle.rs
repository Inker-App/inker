//! Parsing the `handle` argument agents pass to tools.

use inker_core::{Codename, Handle};

/// Accept `🐙/x`, `ink/x` or a bare codename `x`.
pub fn parse_handle_arg(s: &str) -> inker_core::Result<Handle> {
    let trimmed = s.trim();
    if let Ok(h) = Handle::parse(trimmed) {
        return Ok(h);
    }
    Codename::parse(trimmed)
        .map(Handle::new)
        .map_err(|_| inker_core::Error::InvalidHandle(trimmed.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_octopus_ascii_and_bare() {
        for s in [
            "🐙/w2-acme-2025",
            "ink/w2-acme-2025",
            "w2-acme-2025",
            "  🐙/w2-acme-2025\n",
        ] {
            let h = parse_handle_arg(s).unwrap();
            assert_eq!(h.codename.as_str(), "w2-acme-2025", "{s}");
        }
    }

    #[test]
    fn rejects_garbage() {
        for s in ["", "🐙/", "W2", "a--b", "🐙/a b", "../etc/passwd"] {
            assert!(parse_handle_arg(s).is_err(), "{s}");
        }
    }
}

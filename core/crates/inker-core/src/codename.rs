//! Codename proposals: the human-readable part of a handle.
//!
//! `w2-acme-2025`, `1099-int-chase-2025`, `receipt-4471` (no org, no year),
//! `folder-4471` (a folder), `w2-acme-2025-summary` (a `write` output).
//! Collisions get `-2`, `-3`, ... (decision 5.7). Every function returns a
//! valid [`Codename`] built through [`Codename::parse`].

use crate::{Codename, DocKind};

/// Longest codename we propose, leaving room for a collision suffix under the
/// 64-character limit.
const MAX_LEN: usize = 60;
const MAX_KIND: usize = 20;
const MAX_ORG: usize = 20;
const MAX_ORG_JOINED: usize = 16;

/// Words dropped from organisation names.
const ORG_STOPWORDS: &[&str] = &[
    "the",
    "inc",
    "incorporated",
    "llc",
    "corp",
    "corporation",
    "co",
    "company",
    "ltd",
    "limited",
    "lp",
    "llp",
    "plc",
    "na",
    "pc",
    "pllc",
];

/// What goes into a document codename. `TypeYear` is the default because a
/// handle is pasted into agent chats in the clear, and any name in it is a
/// leak (decision 5.16); `TypeOrgYear` (the brief's original) stays available.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CodenamePolicy {
    /// `w2-acme-2025`. Puts the organisation's name in the handle.
    TypeOrgYear,
    /// `w2-2025`. No organisation in the handle.
    #[default]
    TypeYear,
}

/// Lowercase ASCII words. Apostrophes and non-ASCII letters are dropped,
/// every other non-alphanumeric character separates words.
fn words(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            cur.push(c.to_ascii_lowercase());
        } else if c == '\'' || c == '\u{2019}' || (c.is_alphanumeric() && !c.is_ascii()) {
            // dropped, word continues
        } else if !cur.is_empty() {
            out.push(std::mem::take(&mut cur));
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

fn truncate(s: &str, max: usize) -> String {
    let t: String = s.chars().take(max).collect();
    t.trim_end_matches('-').to_string()
}

/// Four-digit discriminator for codenames with nothing else to tell them apart.
fn serial(d: u32) -> u32 {
    if (1000..=9999).contains(&d) {
        d
    } else {
        1000 + d % 9000
    }
}

fn finish(s: &str, fallback: &str) -> Codename {
    let s = truncate(s, MAX_LEN);
    Codename::parse(&s)
        .or_else(|_| Codename::parse(fallback))
        .unwrap_or_else(|_| unreachable_codename())
}

#[allow(clippy::expect_used)]
fn unreachable_codename() -> Codename {
    Codename::parse("doc").expect("\"doc\" is a valid codename")
}

/// Slug for an organisation name: lowercase, digits, punctuation and legal
/// suffixes (Inc, LLC, Corp, ...) dropped, words joined by hyphens; when that
/// is longer than 16 characters, only the first word. `None` when nothing is
/// left. Digits are dropped so an EIN or account number caught in the org
/// span never reaches a handle.
pub fn slugify_org(name: &str) -> Option<String> {
    let ws: Vec<String> = words(name)
        .into_iter()
        .map(|w| {
            w.chars()
                .filter(|c| !c.is_ascii_digit())
                .collect::<String>()
        })
        .filter(|w| !w.is_empty() && !ORG_STOPWORDS.contains(&w.as_str()))
        .collect();
    let first = ws.first()?;
    let joined = ws.join("-");
    Some(if joined.len() > MAX_ORG_JOINED {
        truncate(first, MAX_ORG)
    } else {
        joined
    })
}

/// Slug for a document kind: lowercase words joined by hyphens, a single
/// letter glued to the number after it. `W-2` -> `w2`, `1099-INT` ->
/// `1099-int`, `k1-1065` -> `k1-1065`, `bank-statement` -> `bank-statement`.
/// Unknown or empty kinds become `doc`.
pub fn kind_slug(kind: &DocKind) -> String {
    let parts: Vec<String> = kind
        .as_str()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_ascii_lowercase)
        .collect();
    let mut s = String::new();
    let mut prev_letter = false;
    for p in &parts {
        let glue = prev_letter && p.starts_with(|c: char| c.is_ascii_digit());
        if !s.is_empty() && !glue {
            s.push('-');
        }
        s.push_str(p);
        prev_letter = p.len() == 1 && p.starts_with(|c: char| c.is_ascii_alphabetic());
    }
    if s.is_empty() || s == DocKind::UNKNOWN {
        "doc".into()
    } else {
        truncate(&s, MAX_KIND)
    }
}

/// Propose a document codename. `discriminator` (any number, e.g. derived from
/// the document id) is used only when neither org nor year made it into the
/// name, giving `receipt-4471`. The result is not yet checked for collisions;
/// pass it through [`disambiguate`].
pub fn propose(
    policy: CodenamePolicy,
    kind: &DocKind,
    org: Option<&str>,
    year: Option<u16>,
    discriminator: u32,
) -> Codename {
    let mut parts = vec![kind_slug(kind)];
    if policy == CodenamePolicy::TypeOrgYear
        && let Some(o) = org.and_then(slugify_org)
    {
        parts.push(o);
    }
    if let Some(y) = year {
        parts.push(y.to_string());
    }
    if parts.len() == 1 {
        parts.push(serial(discriminator).to_string());
    }
    finish(&parts.join("-"), "doc")
}

/// Propose a folder codename: `folder-NNNN`. Never built from the folder's
/// name, which is often a person's name. The folder handle exists before its
/// children are classified and may be copied as soon as one child is ready, so
/// it is not renamed later from the children's kinds or years.
pub fn propose_folder(discriminator: u32) -> Codename {
    finish(&format!("folder-{}", serial(discriminator)), "folder")
}

/// Codename for a `write` output: `<source>-<file stem slug>`.
pub fn propose_output(source: &Codename, file_name: &str) -> Codename {
    let last = file_name.rsplit(['/', '\\']).next().unwrap_or(file_name);
    let stem = match last.rsplit_once('.') {
        Some((stem, _)) if !stem.is_empty() => stem,
        _ => last,
    };
    let mut slug = truncate(&words(stem).join("-"), MAX_ORG);
    if slug.is_empty() {
        slug = "output".into();
    }
    let room = MAX_LEN.saturating_sub(slug.len() + 1);
    let src = truncate(source.as_str(), room);
    finish(&format!("{src}-{slug}"), source.as_str())
}

/// `base` if free, else `base-2`, `base-3`, ... (decision 5.7).
pub fn disambiguate(base: Codename, taken: impl Fn(&str) -> bool) -> Codename {
    if !taken(base.as_str()) {
        return base;
    }
    let mut n: u32 = 2;
    loop {
        let candidate = format!("{}-{n}", base.as_str());
        if !taken(&candidate)
            && let Ok(c) = Codename::parse(&candidate)
        {
            return c;
        }
        n += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn k(s: &str) -> DocKind {
        DocKind(s.into())
    }

    #[test]
    fn type_org_year() {
        let c = propose(
            CodenamePolicy::TypeOrgYear,
            &k("w2"),
            Some("Acme Inc."),
            Some(2025),
            1,
        );
        assert_eq!(c.as_str(), "w2-acme-2025");
        let c = propose(
            CodenamePolicy::TypeOrgYear,
            &k("1099-INT"),
            Some("Chase"),
            Some(2025),
            1,
        );
        assert_eq!(c.as_str(), "1099-int-chase-2025");
    }

    #[test]
    fn kind_slugs_keep_the_hyphens_that_make_them_readable() {
        for (kind, slug) in [
            ("w2", "w2"),
            ("W-2", "w2"),
            ("1098", "1098"),
            ("1095-A", "1095-a"),
            ("1099-int", "1099-int"),
            ("1099-NEC", "1099-nec"),
            ("k1-1065", "k1-1065"),
            ("k1-1120s", "k1-1120s"),
            ("K-1 1065", "k1-1065"),
            ("bank-statement", "bank-statement"),
            ("closing statement", "closing-statement"),
            ("receipt", "receipt"),
            ("--odd--kind--", "odd-kind"),
        ] {
            assert_eq!(kind_slug(&k(kind)), slug, "{kind}");
        }
    }

    #[test]
    fn type_year_drops_org() {
        let c = propose(
            CodenamePolicy::TypeYear,
            &k("W-2"),
            Some("Acme Inc."),
            Some(2025),
            1,
        );
        assert_eq!(c.as_str(), "w2-2025");
    }

    #[test]
    fn no_org_no_year_gets_serial() {
        let c = propose(CodenamePolicy::TypeOrgYear, &k("receipt"), None, None, 4471);
        assert_eq!(c.as_str(), "receipt-4471");
        // Serial is folded into four digits.
        let c = propose(
            CodenamePolicy::TypeOrgYear,
            &k("receipt"),
            None,
            None,
            123_456_789,
        );
        assert_eq!(c.as_str().len(), "receipt-".len() + 4);
        // TypeYear with no year also gets a serial even if an org exists.
        let c = propose(
            CodenamePolicy::TypeYear,
            &k("receipt"),
            Some("Acme"),
            None,
            12,
        );
        assert_eq!(c.as_str(), "receipt-1012");
    }

    #[test]
    fn unknown_or_empty_kind_is_doc() {
        let c = propose(CodenamePolicy::TypeOrgYear, &k("???"), None, Some(2024), 1);
        assert_eq!(c.as_str(), "doc-2024");
        assert_eq!(kind_slug(&DocKind::unknown()), "doc");
    }

    #[test]
    fn slugify_org_rules() {
        assert_eq!(slugify_org("Acme, Inc.").as_deref(), Some("acme"));
        assert_eq!(
            slugify_org("Acme Widgets LLC").as_deref(),
            Some("acme-widgets")
        );
        assert_eq!(
            slugify_org("The Placeholder Corp").as_deref(),
            Some("placeholder")
        );
        assert_eq!(
            slugify_org("O'Fakely & Sons Co").as_deref(),
            Some("ofakely-sons")
        );
        assert_eq!(
            slugify_org("Consolidated Imaginary Holdings Group Ltd").as_deref(),
            Some("consolidated")
        );
        assert_eq!(slugify_org("Inc."), None);
        assert_eq!(slugify_org("  "), None);
        assert_eq!(slugify_org("Ünïcode Bank").as_deref(), Some("ncode-bank"));
    }

    #[test]
    fn long_inputs_stay_valid() {
        let long = "x".repeat(200);
        let c = propose(
            CodenamePolicy::TypeOrgYear,
            &k(&long),
            Some(&long),
            Some(2025),
            1,
        );
        assert!(c.as_str().len() <= 60);
    }

    #[test]
    fn folder_codenames_never_come_from_the_folder_name() {
        assert_eq!(propose_folder(1234).as_str(), "folder-1234");
        assert_eq!(propose_folder(7).as_str(), "folder-1007");
    }

    #[test]
    fn default_policy_is_type_year() {
        assert_eq!(CodenamePolicy::default(), CodenamePolicy::TypeYear);
    }

    #[test]
    fn org_slugs_drop_digits() {
        assert_eq!(slugify_org("ACME 07-3456789").as_deref(), Some("acme"));
        assert_eq!(
            slugify_org("Acme 4471 Widgets").as_deref(),
            Some("acme-widgets")
        );
        assert_eq!(slugify_org("07-3456789"), None);
    }

    #[test]
    fn output_names() {
        let src = Codename::parse("w2-acme-2025").unwrap();
        assert_eq!(
            propose_output(&src, "Summary.md").as_str(),
            "w2-acme-2025-summary"
        );
        assert_eq!(
            propose_output(&src, "notes/q1 draft.txt").as_str(),
            "w2-acme-2025-q1-draft"
        );
        assert_eq!(propose_output(&src, "...").as_str(), "w2-acme-2025-output");
    }

    #[test]
    fn collisions_get_suffixes() {
        let base = Codename::parse("w2-acme-2025").unwrap();
        let taken: HashSet<&str> = ["w2-acme-2025", "w2-acme-2025-2"].into_iter().collect();
        assert_eq!(
            disambiguate(base.clone(), |c| taken.contains(c)).as_str(),
            "w2-acme-2025-3"
        );
        assert_eq!(disambiguate(base, |_| false).as_str(), "w2-acme-2025");
    }
}

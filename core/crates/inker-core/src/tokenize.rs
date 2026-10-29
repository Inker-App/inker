//! Apply treatments to extracted text.
//!
//! The detect crate finds spans and kinds; this module decides what replaces
//! them. `Tokenize` mints through the vault, `Generalize` uses the
//! detection's `generalized` text (failing closed to a token when it is
//! missing), `Keep` leaves the text alone. Overlapping detections are resolved
//! by source precision (Template > Rule > Model > FailClosed), then length,
//! and widened rather than dropped (see [`resolve_overlaps`]). Replacements
//! run from the end of the text backwards so earlier offsets stay valid.
//! A second pass sweeps every page and field for other occurrences of the
//! values just tokenized.

use std::ops::Range;

use crate::vault::{Vault, find_tokens, normalize};
use crate::{
    Detection, DetectionSource, Error, Extracted, FormField, KindTreatment, ResolvedPage, Result,
    Span, Token, TokenKind, TokenRole, Treatment,
};

/// Output of [`tokenize`]. Contains tokens and generalized text only.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Tokenized {
    pub pages: Vec<ResolvedPage>,
    /// Form fields with their values treated.
    pub fields: Vec<FormField>,
    /// Token -> role, for `describe`.
    pub roles: Vec<TokenRole>,
    /// Count per (kind, treatment actually applied).
    pub treatments: Vec<KindTreatment>,
    /// Number of spans tokenized or generalized ("6 inked").
    pub inked: u32,
}

/// Lower is more precise.
pub fn source_rank(source: &DetectionSource) -> u8 {
    match source {
        DetectionSource::Template { .. } => 0,
        DetectionSource::Rule { .. } => 1,
        DetectionSource::Model { .. } => 2,
        DetectionSource::FailClosed { .. } => 3,
    }
}

fn overlaps(a: Span, b: Span) -> bool {
    a.start < b.end && b.start < a.end
}

fn contains(outer: Span, inner: Span) -> bool {
    outer.start <= inner.start && inner.end <= outer.end
}

/// `a` minus `b`: zero, one or two spans.
fn subtract(a: Span, b: Span) -> Vec<Span> {
    if !overlaps(a, b) {
        return vec![a];
    }
    let mut out = Vec::new();
    if a.start < b.start {
        out.push(Span {
            start: a.start,
            end: b.start,
        });
    }
    if b.end < a.end {
        out.push(Span {
            start: b.end,
            end: a.end,
        });
    }
    out
}

/// Shrink `s` to exclude leading and trailing whitespace of `text`.
fn trim(text: &str, s: Span) -> Span {
    let inner = &text[s.start..s.end];
    let lead = inner.len() - inner.trim_start().len();
    let trail = inner.len() - inner.trim_end().len();
    if lead == inner.len() {
        return Span {
            start: s.start,
            end: s.start,
        };
    }
    Span {
        start: s.start + lead,
        end: s.end - trail,
    }
}

/// One span to treat, after overlaps are resolved.
#[derive(Debug, Clone, PartialEq)]
pub struct Piece {
    pub span: Span,
    pub kind: TokenKind,
    pub role: Option<String>,
    pub treatment: Treatment,
    /// Replacement for `Generalize`.
    pub generalized: Option<String>,
}

/// From detections over one `text`, a non-overlapping set of pieces to treat.
///
/// Detections are taken most precise source first, then longest, then
/// earliest. A detection inside an earlier piece adds nothing. One that
/// partly overlaps (or contains) an earlier inked piece widens it: the union
/// is tokenized with the kind and role of the more precise piece, so nothing
/// either stage saw is left in the clear. An earlier `Keep` piece (e.g. a
/// "Leave it" answer) keeps its own text, but the parts of a less precise
/// detection outside it are still inked.
pub fn resolve_overlaps(text: &str, dets: &[&Detection]) -> Vec<Piece> {
    let mut sorted: Vec<&Detection> = dets
        .iter()
        .copied()
        .filter(|d| !d.span.is_empty())
        .collect();
    sorted.sort_by(|a, b| {
        source_rank(&a.source)
            .cmp(&source_rank(&b.source))
            .then(b.span.len().cmp(&a.span.len()))
            .then(a.span.start.cmp(&b.span.start))
    });
    let mut pieces: Vec<Piece> = Vec::new();
    for d in sorted {
        let t = effective_treatment(d);
        if pieces.iter().any(|p| contains(p.span, d.span)) {
            continue;
        }
        if t == Treatment::Keep {
            if !pieces.iter().any(|p| overlaps(p.span, d.span)) {
                pieces.push(Piece {
                    span: d.span,
                    kind: d.kind.clone(),
                    role: d.role.clone(),
                    treatment: t,
                    generalized: None,
                });
            }
            continue;
        }
        let mut parts = vec![d.span];
        for k in pieces.iter().filter(|p| p.treatment == Treatment::Keep) {
            parts = parts
                .into_iter()
                .flat_map(|x| subtract(x, k.span))
                .collect();
        }
        let whole = parts.len() == 1 && parts[0] == d.span;
        for part in parts {
            let part = trim(text, part);
            if part.is_empty() {
                continue;
            }
            let hit: Vec<usize> = pieces
                .iter()
                .enumerate()
                .filter(|(_, p)| p.treatment != Treatment::Keep && overlaps(p.span, part))
                .map(|(i, _)| i)
                .collect();
            let Some(&first) = hit.first() else {
                // A cut-down span no longer matches its generalized text.
                let treatment = if whole { t } else { Treatment::Tokenize };
                pieces.push(Piece {
                    span: part,
                    kind: d.kind.clone(),
                    role: d.role.clone(),
                    treatment,
                    generalized: (treatment == Treatment::Generalize)
                        .then(|| d.generalized.clone())
                        .flatten(),
                });
                continue;
            };
            if hit.len() == 1 && contains(pieces[first].span, part) {
                continue;
            }
            let start = hit
                .iter()
                .map(|&i| pieces[i].span.start)
                .chain([part.start])
                .min()
                .unwrap_or(part.start);
            let end = hit
                .iter()
                .map(|&i| pieces[i].span.end)
                .chain([part.end])
                .max()
                .unwrap_or(part.end);
            pieces[first] = Piece {
                span: Span { start, end },
                treatment: Treatment::Tokenize,
                generalized: None,
                ..pieces[first].clone()
            };
            for &i in hit[1..].iter().rev() {
                pieces.remove(i);
            }
        }
    }
    pieces
}

fn effective_treatment(d: &Detection) -> Treatment {
    match d.treatment {
        Treatment::Generalize if d.generalized.is_none() => Treatment::Tokenize,
        t => t,
    }
}

fn check_span(text: &str, d: &Detection, what: &str) -> Result<()> {
    let s = d.span;
    if s.end > text.len()
        || s.start > s.end
        || !text.is_char_boundary(s.start)
        || !text.is_char_boundary(s.end)
    {
        return Err(Error::Detect(format!(
            "{} span {}..{} does not fit {what} (len {})",
            d.kind.label(),
            s.start,
            s.end,
            text.len()
        )));
    }
    Ok(())
}

/// Shortest normalised value the sweep looks for elsewhere: names and other
/// text under 4 characters, and numbers under 6, match too much by chance.
const MIN_SWEEP_TEXT: usize = 4;
const MIN_SWEEP_NUMBER: usize = 6;
/// Shortest word of a minted name swept on its own: initials and two-letter
/// particles match too much by chance.
const MIN_NAME_WORD: usize = 3;

/// Characters allowed between the digits of a swept number.
fn is_number_separator(c: char) -> bool {
    matches!(c, ' ' | '-' | '.' | '/' | '(' | ')' | '\u{a0}' | '\t')
}

/// How a minted value is found again elsewhere in its document.
enum Matcher {
    /// Alphanumerics compared upper-cased, separators skipped.
    Number(Vec<char>),
    /// Case-folded, any run of whitespace matching one space.
    Text(Vec<char>),
    /// One word of a minted name, case-folded, matched only where the text
    /// starts it with a capital ("Example", "EXAMPLE", not "example").
    Word(Vec<char>),
}

impl Matcher {
    fn for_value(kind: &TokenKind, value: &str) -> Option<Self> {
        let key = normalize(kind, value);
        match kind {
            TokenKind::Ssn
            | TokenKind::Itin
            | TokenKind::Ein
            | TokenKind::Account
            | TokenKind::Routing
            | TokenKind::Card
            | TokenKind::GovId
            | TokenKind::Phone => {
                // A phone with an extension is not swept: its base number
                // alone may be a different value.
                let chars: Vec<char> = key.chars().collect();
                let ok = chars.len() >= MIN_SWEEP_NUMBER
                    && !(matches!(kind, TokenKind::Phone) && key.contains('x'));
                ok.then_some(Matcher::Number(chars))
            }
            TokenKind::Person
            | TokenKind::Org
            | TokenKind::Address
            | TokenKind::Email
            | TokenKind::Custom(_) => {
                let chars: Vec<char> = key.chars().collect();
                (chars.len() >= MIN_SWEEP_TEXT).then_some(Matcher::Text(chars))
            }
        }
    }

    /// The words of a Person value that the sweep also looks for alone:
    /// runs of 3+ letters (apostrophes kept), each once.
    fn name_words(value: &str) -> Vec<Self> {
        let mut words: Vec<Vec<char>> = Vec::new();
        for w in value.split(|c: char| !(c.is_alphabetic() || c == '\'' || c == '\u{2019}')) {
            let w = w.trim_matches(|c: char| !c.is_alphabetic());
            if w.chars().filter(|c| c.is_alphabetic()).count() < MIN_NAME_WORD {
                continue;
            }
            let key: Vec<char> = w.chars().flat_map(char::to_lowercase).collect();
            if !words.contains(&key) {
                words.push(key);
            }
        }
        words.into_iter().map(Matcher::Word).collect()
    }

    fn len(&self) -> usize {
        match self {
            Matcher::Number(k) | Matcher::Text(k) | Matcher::Word(k) => k.len(),
        }
    }

    /// End of a match starting at byte `start`, if one does.
    fn match_at(&self, text: &str, start: usize) -> Option<usize> {
        let mut k = 0;
        let mut in_ws = false;
        for (off, c) in text[start..].char_indices() {
            match self {
                Matcher::Number(key) => {
                    if c.is_alphanumeric() {
                        for u in c.to_uppercase() {
                            if key.get(k) != Some(&u) {
                                return None;
                            }
                            k += 1;
                        }
                    } else if k > 0 && is_number_separator(c) {
                        continue;
                    } else {
                        return None;
                    }
                    if k == key.len() {
                        return Some(start + off + c.len_utf8());
                    }
                }
                Matcher::Text(key) | Matcher::Word(key) => {
                    if k == 0 && matches!(self, Matcher::Word(_)) && !c.is_uppercase() {
                        return None;
                    }
                    if c.is_whitespace() {
                        if in_ws {
                            continue;
                        }
                        if k > 0 && key.get(k) == Some(&' ') {
                            k += 1;
                            in_ws = true;
                            continue;
                        }
                        return None;
                    }
                    in_ws = false;
                    for l in c.to_lowercase() {
                        if key.get(k) != Some(&l) {
                            return None;
                        }
                        k += 1;
                    }
                    if k == key.len() {
                        return Some(start + off + c.len_utf8());
                    }
                }
            }
        }
        None
    }

    /// Every non-overlapping match in `text` on word boundaries, outside
    /// `skip` (existing tokens).
    fn find_all(&self, text: &str, skip: &[Range<usize>]) -> Vec<Range<usize>> {
        let mut out = Vec::new();
        let mut prev: Option<char> = None;
        let mut from = 0;
        for (i, c) in text.char_indices() {
            let boundary = !prev.is_some_and(char::is_alphanumeric);
            prev = Some(c);
            if i < from || !boundary || !c.is_alphanumeric() {
                continue;
            }
            let Some(end) = self.match_at(text, i) else {
                continue;
            };
            if text[end..]
                .chars()
                .next()
                .is_some_and(char::is_alphanumeric)
            {
                continue;
            }
            if skip.iter().any(|r| r.start < end && i < r.end) {
                continue;
            }
            out.push(i..end);
            from = end;
        }
        out
    }
}

struct Acc<'v> {
    vault: &'v Vault,
    roles: Vec<TokenRole>,
    treatments: Vec<KindTreatment>,
    inked: u32,
    /// Every value tokenized in this document, for the sweep.
    minted: Vec<(TokenKind, String, Token)>,
}

impl Acc<'_> {
    fn count(&mut self, kind: &TokenKind, treatment: Treatment) {
        if let Some(kt) = self
            .treatments
            .iter_mut()
            .find(|k| &k.kind == kind && k.treatment == treatment)
        {
            kt.count += 1;
        } else {
            self.treatments.push(KindTreatment {
                kind: kind.clone(),
                treatment,
                count: 1,
            });
        }
    }

    /// Apply the resolved pieces of `dets` to `text`.
    fn apply(&mut self, text: &str, dets: &[&Detection]) -> Result<String> {
        let mut pieces = resolve_overlaps(text, dets);
        pieces.sort_by_key(|p| std::cmp::Reverse(p.span.start));
        let mut out = text.to_string();
        for p in pieces {
            self.count(&p.kind, p.treatment);
            let replacement = match p.treatment {
                Treatment::Keep => continue,
                Treatment::Generalize => p.generalized.clone().unwrap_or_default(),
                Treatment::Tokenize => {
                    let value = &text[p.span.start..p.span.end];
                    let token = self.vault.mint(&p.kind, value)?;
                    if let Some(role) = &p.role
                        && !self
                            .roles
                            .iter()
                            .any(|r| r.token == token && &r.role == role)
                    {
                        self.roles.push(TokenRole {
                            token: token.clone(),
                            role: role.clone(),
                        });
                    }
                    if !self.minted.iter().any(|(_, _, t)| *t == token) {
                        self.minted
                            .push((p.kind.clone(), value.to_string(), token.clone()));
                    }
                    token.to_string()
                }
            };
            out.replace_range(p.span.start..p.span.end, &replacement);
            self.inked += 1;
        }
        Ok(out)
    }

    /// Replace every other occurrence of a value minted in this document,
    /// in any spelling that normalises the same, with its token.
    fn sweep(&mut self, text: &str) -> String {
        let mut matchers: Vec<(Matcher, TokenKind, String)> = self
            .minted
            .iter()
            .filter_map(|(k, v, t)| Matcher::for_value(k, v).map(|m| (m, k.clone(), t.to_string())))
            .collect();
        // Each word of a minted name on its own, so a name split over
        // columns or reordered ("Example, Jordan A") never leaves a part in
        // the clear beside its token.
        for (k, v, t) in &self.minted {
            if *k == TokenKind::Person {
                matchers.extend(
                    Matcher::name_words(v)
                        .into_iter()
                        .map(|m| (m, k.clone(), t.to_string())),
                );
            }
        }
        // Longer values first, so a full name wins over a part of it. The
        // sort is stable: a whole value is tried before a word of the same
        // length.
        matchers.sort_by_key(|(m, _, _)| std::cmp::Reverse(m.len()));
        let mut out = text.to_string();
        for (m, kind, token) in matchers {
            let skip: Vec<Range<usize>> =
                find_tokens(&out).into_iter().map(|(r, _, _)| r).collect();
            let hits = m.find_all(&out, &skip);
            for r in hits.into_iter().rev() {
                out.replace_range(r, &token);
                self.count(&kind, Treatment::Tokenize);
                self.inked += 1;
            }
        }
        out
    }
}

/// Apply every detection's treatment to the pages and form fields of
/// `extracted`. Spans that do not fit their text are an error (fail closed):
/// nothing is minted in that case.
pub fn tokenize(
    vault: &Vault,
    extracted: &Extracted,
    detections: &[Detection],
) -> Result<Tokenized> {
    // Validate everything before minting anything.
    for d in detections {
        match &d.field {
            Some(name) => {
                let matching: Vec<&FormField> = extracted
                    .fields
                    .iter()
                    .filter(|f| &f.name == name)
                    .collect();
                if matching.is_empty() {
                    return Err(Error::Detect(format!(
                        "detection names a form field that does not exist ({})",
                        d.kind.label()
                    )));
                }
                for f in matching {
                    check_span(&f.value, d, "form field")?;
                }
            }
            None => {
                let p = extracted
                    .pages
                    .iter()
                    .find(|p| p.page == d.page)
                    .ok_or_else(|| {
                        Error::Detect(format!(
                            "detection on page {} which was not extracted",
                            d.page
                        ))
                    })?;
                check_span(&p.text, d, "page")?;
            }
        }
    }

    let mut acc = Acc {
        vault,
        roles: Vec::new(),
        treatments: Vec::new(),
        inked: 0,
        minted: Vec::new(),
    };
    let mut pages = Vec::with_capacity(extracted.pages.len());
    for p in &extracted.pages {
        let dets: Vec<&Detection> = detections
            .iter()
            .filter(|d| d.field.is_none() && d.page == p.page)
            .collect();
        pages.push(ResolvedPage {
            page: p.page,
            text: acc.apply(&p.text, &dets)?,
        });
    }
    let mut fields = Vec::with_capacity(extracted.fields.len());
    for f in &extracted.fields {
        let dets: Vec<&Detection> = detections
            .iter()
            .filter(|d| d.field.as_deref() == Some(f.name.as_str()))
            .collect();
        fields.push(FormField {
            name: f.name.clone(),
            value: acc.apply(&f.value, &dets)?,
            page: f.page,
        });
    }
    // Second pass: other occurrences of the values just inked, anywhere in
    // the document, including spellings the detectors did not flag.
    for p in &mut pages {
        p.text = acc.sweep(&p.text);
    }
    for f in &mut fields {
        f.value = acc.sweep(&f.value);
    }
    Ok(Tokenized {
        pages,
        fields,
        roles: acc.roles,
        treatments: acc.treatments,
        inked: acc.inked,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault::MemoryKeyStore;
    use crate::{Error, PageText, Span, TextOrigin, TokenKind, Treatment};
    use camino::Utf8PathBuf;

    fn vault() -> (tempfile::TempDir, Vault) {
        let d = tempfile::tempdir().unwrap();
        let p = Utf8PathBuf::from_path_buf(d.path().join("vault.bin")).unwrap();
        let v = Vault::open(&p, &MemoryKeyStore::new().unwrap()).unwrap();
        (d, v)
    }

    fn page(n: u32, text: &str) -> PageText {
        PageText {
            page: n,
            text: text.into(),
            origin: TextOrigin::TextLayer,
        }
    }

    fn det(
        page: u32,
        text: &str,
        needle: &str,
        kind: TokenKind,
        source: DetectionSource,
        t: Treatment,
    ) -> Detection {
        let start = text.find(needle).unwrap();
        Detection {
            page,
            span: Span {
                start,
                end: start + needle.len(),
            },
            kind,
            source,
            confidence: 0.9,
            treatment: t,
            needs_review: false,
            review_prompt: None,
            role: None,
            generalized: None,
            field: None,
        }
    }

    fn rule() -> DetectionSource {
        DetectionSource::Rule { name: "r".into() }
    }
    fn template() -> DetectionSource {
        DetectionSource::Template {
            form: "w2".into(),
            field: "a".into(),
        }
    }
    fn model() -> DetectionSource {
        DetectionSource::Model { name: "m".into() }
    }
    fn fail_closed() -> DetectionSource {
        DetectionSource::FailClosed {
            reason: "digits".into(),
        }
    }

    #[test]
    fn ranks() {
        assert!(source_rank(&template()) < source_rank(&rule()));
        assert!(source_rank(&rule()) < source_rank(&model()));
        assert!(source_rank(&model()) < source_rank(&fail_closed()));
    }

    #[test]
    fn tokenize_generalize_keep() {
        let (_d, v) = vault();
        let text = "SSN 000-12-3456 lives at 1 Fake St, Brooklyn NY; wages 1000";
        let mut g = det(
            1,
            text,
            "1 Fake St, Brooklyn NY",
            TokenKind::Address,
            template(),
            Treatment::Generalize,
        );
        g.generalized = Some("Brooklyn, NY".into());
        let mut s = det(
            1,
            text,
            "000-12-3456",
            TokenKind::Ssn,
            rule(),
            Treatment::Tokenize,
        );
        s.role = Some("employee".into());
        let k = det(
            1,
            text,
            "1000",
            TokenKind::Custom("amount".into()),
            template(),
            Treatment::Keep,
        );
        let e = Extracted {
            pages: vec![page(1, text)],
            fields: vec![],
        };
        let out = tokenize(&v, &e, &[s, g, k]).unwrap();
        assert_eq!(
            out.pages[0].text,
            "SSN [SSN_1] lives at Brooklyn, NY; wages 1000"
        );
        assert_eq!(out.inked, 2);
        assert_eq!(out.roles.len(), 1);
        assert_eq!(out.roles[0].token.to_string(), "[SSN_1]");
        assert_eq!(out.roles[0].role, "employee");
        assert_eq!(out.treatments.len(), 3);
        assert!(out.treatments.iter().any(|t| t.kind == TokenKind::Ssn
            && t.treatment == Treatment::Tokenize
            && t.count == 1));
    }

    #[test]
    fn generalize_without_replacement_fails_closed() {
        let (_d, v) = vault();
        let text = "at 1 Fake St";
        let g = det(
            1,
            text,
            "1 Fake St",
            TokenKind::Address,
            model(),
            Treatment::Generalize,
        );
        let e = Extracted {
            pages: vec![page(1, text)],
            fields: vec![],
        };
        let out = tokenize(&v, &e, &[g]).unwrap();
        assert_eq!(out.pages[0].text, "at [ADDRESS_1]");
        assert_eq!(out.treatments[0].treatment, Treatment::Tokenize);
    }

    fn one_page(text: &str) -> Extracted {
        Extracted {
            pages: vec![page(1, text)],
            fields: vec![],
        }
    }

    #[test]
    fn overlap_prefers_precision_then_length() {
        let (_d, v) = vault();
        let text = "Pay Testy McFakename now";
        // Same source, one inside the other: the longer one alone.
        let a = det(
            1,
            text,
            "Testy",
            TokenKind::Person,
            model(),
            Treatment::Tokenize,
        );
        let b = det(
            1,
            text,
            "Testy McFakename",
            TokenKind::Person,
            model(),
            Treatment::Tokenize,
        );
        let out = tokenize(&v, &one_page(text), &[a, b]).unwrap();
        assert_eq!(out.pages[0].text, "Pay [PERSON_1] now");
        assert_eq!(v.rehydrate("[PERSON_1]"), "Testy McFakename");
        assert_eq!(out.inked, 1);
    }

    #[test]
    fn partial_overlap_tokenizes_the_union_with_the_precise_kind() {
        let (_d, v) = vault();
        let text = "Pay Testy McFakename now";
        let mut r = det(
            1,
            text,
            "Testy McFakename",
            TokenKind::Person,
            rule(),
            Treatment::Tokenize,
        );
        r.role = Some("payee".into());
        let m = det(
            1,
            text,
            "McFakename now",
            TokenKind::Org,
            model(),
            Treatment::Tokenize,
        );
        let out = tokenize(&v, &one_page(text), &[m.clone(), r.clone()]).unwrap();
        assert_eq!(out.pages[0].text, "Pay [PERSON_1]");
        assert_eq!(v.rehydrate("[PERSON_1]"), "Testy McFakename now");
        assert_eq!(out.roles[0].role, "payee");
        // A less precise span that contains the precise one also widens it.
        let (_d2, v2) = vault();
        let wide = det(
            1,
            text,
            "Testy McFakename now",
            TokenKind::Org,
            model(),
            Treatment::Tokenize,
        );
        let out = tokenize(&v2, &one_page(text), &[wide, r]).unwrap();
        assert_eq!(out.pages[0].text, "Pay [PERSON_1]");
        assert_eq!(v2.rehydrate("[PERSON_1]"), "Testy McFakename now");
    }

    #[test]
    fn keep_winner_still_inks_the_losers_uncovered_tail() {
        let (_d, v) = vault();
        let text = "Pay Testy McFakename now";
        let keep = det(
            1,
            text,
            "Testy",
            TokenKind::Person,
            template(),
            Treatment::Keep,
        );
        let m = det(
            1,
            text,
            "Testy McFakename",
            TokenKind::Person,
            model(),
            Treatment::Tokenize,
        );
        let out = tokenize(&v, &one_page(text), &[m, keep]).unwrap();
        assert_eq!(out.pages[0].text, "Pay Testy [PERSON_1] now");
        assert_eq!(v.rehydrate("[PERSON_1]"), "McFakename");
    }

    #[test]
    fn repeats_of_inked_values_are_swept() {
        let (_d, v) = vault();
        let text = "Employee Testy McFakename, SSN 000-12-3456. \
                    Signed: testy  MCFAKENAME on file 000123456; \
                    not Testy McFakenames or 1000123456.";
        let dets = [
            det(
                1,
                text,
                "Testy McFakename",
                TokenKind::Person,
                rule(),
                Treatment::Tokenize,
            ),
            det(
                1,
                text,
                "000-12-3456",
                TokenKind::Ssn,
                rule(),
                Treatment::Tokenize,
            ),
        ];
        let p2 = "Page two: Testy\nMcFakename, 000 12 3456.";
        let e = Extracted {
            pages: vec![page(1, text), page(2, p2)],
            fields: vec![FormField {
                name: "f1_01".into(),
                value: "Testy McFakename".into(),
                page: Some(1),
            }],
        };
        let out = tokenize(&v, &e, &dets).unwrap();
        assert_eq!(
            out.pages[0].text,
            "Employee [PERSON_1], SSN [SSN_1]. Signed: [PERSON_1] on file [SSN_1]; \
             not [PERSON_1] McFakenames or 1000123456."
        );
        assert_eq!(out.pages[1].text, "Page two: [PERSON_1], [SSN_1].");
        assert_eq!(out.fields[0].value, "[PERSON_1]");
        // The lone "Testy" is a word of the minted name; "McFakenames" is
        // not "McFakename" on a word boundary.
        assert_eq!(out.inked, 2 + 2 + 2 + 1 + 1);
    }

    #[test]
    fn a_minted_names_words_are_swept_when_capitalised() {
        // A name minted whole in one place (here a fillable field) and
        // printed split or reordered elsewhere on the same document: the
        // sweep must still catch each capitalised word of 3+ letters, so a
        // surname can never survive beside a token.
        let (_d, v) = vault();
        let text = "e Employee's first name and initial   Last name\n\
                    [PERSON_1]    McFakename\n\
                    MCFAKENAME, Testy Q. Dear Testy, the mcfakename file is ready. Tes.";
        let e = Extracted {
            pages: vec![page(1, text), page(2, "Signed, McFakename-Jones")],
            fields: vec![FormField {
                name: "f1_02".into(),
                value: "Testy Q McFakename".into(),
                page: Some(1),
            }],
        };
        let mut d = det(
            1,
            "Testy Q McFakename",
            "Testy Q McFakename",
            TokenKind::Person,
            template(),
            Treatment::Tokenize,
        );
        d.field = Some("f1_02".into());
        let out = tokenize(&v, &e, &[d]).unwrap();
        assert_eq!(out.fields[0].value, "[PERSON_1]");
        assert_eq!(
            out.pages[0].text,
            "e Employee's first name and initial   Last name\n\
             [PERSON_1]    [PERSON_1]\n\
             [PERSON_1], [PERSON_1] Q. Dear [PERSON_1], the mcfakename file is ready. Tes."
        );
        assert_eq!(out.pages[1].text, "Signed, [PERSON_1]-Jones");
    }

    #[test]
    fn name_words_are_only_swept_from_person_values() {
        let (_d, v) = vault();
        let text = "Acme Widgets Inc paid. Widgets are fine.";
        let d = det(
            1,
            text,
            "Acme Widgets Inc",
            TokenKind::Org,
            template(),
            Treatment::Tokenize,
        );
        let out = tokenize(&v, &one_page(text), &[d]).unwrap();
        assert_eq!(out.pages[0].text, "[ORG_1] paid. Widgets are fine.");
    }

    #[test]
    fn a_value_seen_in_a_field_is_swept_from_the_pages() {
        let (_d, v) = vault();
        let e = Extracted {
            pages: vec![page(1, "Box a: 000 12 3456 (copy B)")],
            fields: vec![FormField {
                name: "f1_01".into(),
                value: "000-12-3456".into(),
                page: Some(1),
            }],
        };
        let mut fd = det(
            1,
            "000-12-3456",
            "000-12-3456",
            TokenKind::Ssn,
            template(),
            Treatment::Tokenize,
        );
        fd.field = Some("f1_01".into());
        let out = tokenize(&v, &e, &[fd]).unwrap();
        assert_eq!(out.fields[0].value, "[SSN_1]");
        assert_eq!(out.pages[0].text, "Box a: [SSN_1] (copy B)");
    }

    #[test]
    fn short_values_are_not_swept() {
        let (_d, v) = vault();
        let text = "Bo paid 4471; Bo again, 4471 again";
        let dets = [
            det(
                1,
                text,
                "Bo",
                TokenKind::Person,
                rule(),
                Treatment::Tokenize,
            ),
            det(
                1,
                text,
                "4471",
                TokenKind::Account,
                rule(),
                Treatment::Tokenize,
            ),
        ];
        let out = tokenize(&v, &one_page(text), &dets).unwrap();
        assert_eq!(
            out.pages[0].text,
            "[PERSON_1] paid [ACCOUNT_1]; Bo again, 4471 again"
        );
    }

    #[test]
    fn kept_values_are_not_swept() {
        let (_d, v) = vault();
        let text = "Acme Widgets and Acme Widgets";
        let k = det(
            1,
            text,
            "Acme Widgets",
            TokenKind::Org,
            template(),
            Treatment::Keep,
        );
        let out = tokenize(&v, &one_page(text), &[k]).unwrap();
        assert_eq!(out.pages[0].text, text);
    }

    #[test]
    fn multiple_spans_and_pages_keep_offsets() {
        let (_d, v) = vault();
        let p1 = "A 000-11-1111 B 000-22-2222 C 000-11-1111";
        let p2 = "D 000-22-2222";
        let mut dets = vec![
            det(
                1,
                p1,
                "000-11-1111",
                TokenKind::Ssn,
                rule(),
                Treatment::Tokenize,
            ),
            det(
                1,
                p1,
                "000-22-2222",
                TokenKind::Ssn,
                rule(),
                Treatment::Tokenize,
            ),
            det(
                2,
                p2,
                "000-22-2222",
                TokenKind::Ssn,
                rule(),
                Treatment::Tokenize,
            ),
        ];
        let last = p1.rfind("000-11-1111").unwrap();
        dets.push(Detection {
            span: Span {
                start: last,
                end: last + 11,
            },
            ..dets[0].clone()
        });
        let e = Extracted {
            pages: vec![page(1, p1), page(2, p2)],
            fields: vec![],
        };
        let out = tokenize(&v, &e, &dets).unwrap();
        assert_eq!(out.pages[0].text, "A [SSN_1] B [SSN_2] C [SSN_1]");
        assert_eq!(out.pages[1].text, "D [SSN_2]");
        assert_eq!(out.inked, 4);
    }

    #[test]
    fn form_fields_tokenized_by_field() {
        let (_d, v) = vault();
        let e = Extracted {
            pages: vec![page(1, "Employee SSN 000-12-3456")],
            fields: vec![
                FormField {
                    name: "f1_01".into(),
                    value: "000-12-3456".into(),
                    page: Some(1),
                },
                FormField {
                    name: "f1_02".into(),
                    value: "12.00".into(),
                    page: Some(1),
                },
            ],
        };
        let mut fd = det(
            1,
            "000-12-3456",
            "000-12-3456",
            TokenKind::Ssn,
            template(),
            Treatment::Tokenize,
        );
        fd.field = Some("f1_01".into());
        let pd = det(
            1,
            "Employee SSN 000-12-3456",
            "000-12-3456",
            TokenKind::Ssn,
            rule(),
            Treatment::Tokenize,
        );
        let out = tokenize(&v, &e, &[fd, pd]).unwrap();
        assert_eq!(out.fields[0].value, "[SSN_1]");
        assert_eq!(out.fields[1].value, "12.00");
        assert_eq!(out.pages[0].text, "Employee SSN [SSN_1]");
    }

    #[test]
    fn bad_spans_fail_closed() {
        let (_d, v) = vault();
        let e = Extracted {
            pages: vec![page(1, "short")],
            fields: vec![],
        };
        let mut d = det(
            1,
            "short",
            "short",
            TokenKind::Ssn,
            rule(),
            Treatment::Tokenize,
        );
        d.span = Span { start: 2, end: 99 };
        assert!(matches!(
            tokenize(&v, &e, &[d.clone()]),
            Err(Error::Detect(_))
        ));
        d.page = 9;
        d.span = Span { start: 0, end: 1 };
        assert!(matches!(
            tokenize(&v, &e, &[d.clone()]),
            Err(Error::Detect(_))
        ));
        let e2 = Extracted {
            pages: vec![page(1, "é")],
            fields: vec![],
        };
        d.page = 1;
        d.span = Span { start: 0, end: 1 };
        assert!(matches!(
            tokenize(&v, &e2, &[d.clone()]),
            Err(Error::Detect(_))
        ));
        d.field = Some("missing".into());
        assert!(matches!(tokenize(&v, &e, &[d]), Err(Error::Detect(_))));
    }
}

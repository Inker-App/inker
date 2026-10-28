//! Apply treatments to extracted text.
//!
//! The detect crate finds spans and kinds; this module decides what replaces
//! them. `Tokenize` mints through the vault, `Generalize` uses the
//! detection's `generalized` text (failing closed to a token when it is
//! missing), `Keep` leaves the text alone. Overlapping detections are resolved
//! by source precision (Template > Rule > Model > FailClosed), then length.
//! Replacements run from the end of the text backwards so earlier offsets
//! stay valid.

use crate::vault::Vault;
use crate::{
    Detection, DetectionSource, Error, Extracted, FormField, KindTreatment, ResolvedPage, Result,
    TokenKind, TokenRole, Treatment,
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

/// From detections over one text, keep a non-overlapping set: most precise
/// source first, then longest span, then earliest start.
pub fn select_winners<'a>(dets: &[&'a Detection]) -> Vec<&'a Detection> {
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
    let mut won: Vec<&Detection> = Vec::new();
    for d in sorted {
        if won
            .iter()
            .all(|w| d.span.end <= w.span.start || d.span.start >= w.span.end)
        {
            won.push(d);
        }
    }
    won
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

struct Acc<'v> {
    vault: &'v Vault,
    roles: Vec<TokenRole>,
    treatments: Vec<KindTreatment>,
    inked: u32,
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

    /// Apply the winners among `dets` to `text`.
    fn apply(&mut self, text: &str, dets: &[&Detection]) -> Result<String> {
        let mut winners = select_winners(dets);
        winners.sort_by_key(|d| std::cmp::Reverse(d.span.start));
        let mut out = text.to_string();
        for d in winners {
            let t = effective_treatment(d);
            self.count(&d.kind, t);
            let replacement = match t {
                Treatment::Keep => continue,
                Treatment::Generalize => d.generalized.clone().unwrap_or_default(),
                Treatment::Tokenize => {
                    let token = self.vault.mint(&d.kind, &text[d.span.start..d.span.end])?;
                    if let Some(role) = &d.role
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
                    token.to_string()
                }
            };
            out.replace_range(d.span.start..d.span.end, &replacement);
            self.inked += 1;
        }
        Ok(out)
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

    #[test]
    fn overlap_prefers_precision_then_length() {
        let (_d, v) = vault();
        let text = "Pay Testy McFakename now";
        // Model sees a longer span, rule sees a shorter one: rule wins.
        let m = det(
            1,
            text,
            "Testy McFakename now",
            TokenKind::Org,
            model(),
            Treatment::Tokenize,
        );
        let r = det(
            1,
            text,
            "Testy McFakename",
            TokenKind::Person,
            rule(),
            Treatment::Tokenize,
        );
        let e = Extracted {
            pages: vec![page(1, text)],
            fields: vec![],
        };
        let out = tokenize(&v, &e, &[m, r]).unwrap();
        assert_eq!(out.pages[0].text, "Pay [PERSON_1] now");

        // Same source: longer wins.
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
        let refs = [&a, &b];
        let w = select_winners(&refs);
        assert_eq!(w.len(), 1);
        assert_eq!(w[0].span, b.span);
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

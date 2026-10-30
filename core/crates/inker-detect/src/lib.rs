//! inker-detect: finds sensitive values and classifies documents.
//!
//! Input is [`Extracted`](inker_core::Extracted); output is a list of
//! [`Detection`](inker_core::Detection)s plus a [`Classification`]. This crate
//! never mints tokens and never sees the vault. See docs/stack-decision.md
//! section 3.5 for the pipeline it implements.

pub mod address;
pub mod classify;
pub mod dob;
pub mod failclosed;
pub mod fields;
pub mod model;
pub mod names;
pub mod rules;
pub mod same_value;
pub mod shape;
pub mod template;
pub mod unplaced;
mod util;

pub use util::detection;

pub use classify::Classification;
pub use model::{Model, NoModel};

use inker_core::{Detection, DetectionSource, Extracted, PageText, Result, TokenKind, Treatment};
use inker_packs::PackSet;

/// The detection pipeline: templates, rules, names, addresses, dates of
/// birth, the model (if any), then fail-closed and, on form pages, unplaced
/// values; overlaps merged, repeats of a placed number given its kind, roles
/// attached from template fields.
#[derive(Debug)]
pub struct Detector<'a> {
    packs: &'a PackSet,
    model: Box<dyn Model + 'a>,
}

impl<'a> Detector<'a> {
    /// A detector with no model.
    pub fn new(packs: &'a PackSet) -> Self {
        Self {
            packs,
            model: Box::new(NoModel),
        }
    }

    /// Use this model for the ML pass.
    pub fn with_model(mut self, model: Box<dyn Model + 'a>) -> Self {
        self.model = model;
        self
    }

    /// Run the pipeline over one extracted file.
    pub fn detect(&self, extracted: &Extracted) -> Result<(Vec<Detection>, Classification)> {
        let matched = template::match_template(extracted, self.packs);
        let from_template = matched
            .as_ref()
            .map(|m| template::apply(extracted, m))
            .unwrap_or_default();
        tracing::debug!(
            template = matched.as_ref().map(|m| m.template.id.as_str()),
            found = from_template.detections.len(),
            "templates"
        );

        // A form's printed instruction pages hold no typed value: the names
        // and addresses there are examples and agencies. Only the identifier
        // rules run on them.
        let instructions: &[u32] = matched
            .as_ref()
            .map_or(&[], |m| m.instruction_pages.as_slice());
        let mut all = from_template.detections.clone();
        for page in &extracted.pages {
            all.extend(rules::find(page));
            if !instructions.contains(&page.page) {
                all.extend(names::find(page));
                all.extend(address::find(page));
            }
            all.extend(dob::find(page));
        }
        // Every form field value is scanned as a page of its own, whether or
        // not a template maps its name.
        let field_pages: Vec<(&str, PageText)> = extracted
            .fields
            .iter()
            .map(|f| (f.name.as_str(), fields::page(f)))
            .collect();
        for (name, p) in &field_pages {
            let found = rules::find(p)
                .into_iter()
                .chain(names::find(p))
                .chain(address::find(p))
                .chain(dob::find(p));
            all.extend(fields::tag(found, name));
        }
        all.extend(self.model.detect(&extracted.pages)?);
        let mut merged = merge(all);

        for page in &extracted.pages {
            let covered: Vec<(usize, usize)> = merged
                .iter()
                .filter(|d| d.page == page.page && d.field.is_none())
                .map(|d| (d.span.start, d.span.end))
                .chain(
                    from_template
                        .keep
                        .iter()
                        .filter(|(p, _)| *p == page.page)
                        .map(|(_, s)| (s.start, s.end)),
                )
                .collect();
            let form = matched.as_ref().filter(|m| m.pages.contains(&page.page));
            // On a form page, box labels are fixed wording: a "name" near
            // a label that is itself label text is not held.
            let labels =
                form.map_or_else(Vec::new, |m| template::label_spans(&page.text, m.template));
            let fc: Vec<Detection> = failclosed::find(page, &covered)
                .into_iter()
                .filter(|d| !util::overlaps(&labels, d.span.start, d.span.end))
                .collect();
            // On a form page, what is still unplaced and shaped like a value
            // is held too: IRS text layers list values apart from labels.
            if let Some(m) = form {
                let mut covered = covered;
                covered.extend(fc.iter().map(|d| (d.span.start, d.span.end)));
                merged.extend(unplaced::find(page, m.template, self.packs, &covered));
            }
            merged.extend(fc);
        }
        let mut held = Vec::new();
        for (name, p) in &field_pages {
            let mut covered: Vec<(usize, usize)> = merged
                .iter()
                .filter(|d| d.field.as_deref() == Some(*name))
                .map(|d| (d.span.start, d.span.end))
                .collect();
            let fc = fields::tag(failclosed::find(p, &covered), name);
            covered.extend(fc.iter().map(|d| (d.span.start, d.span.end)));
            held.extend(fc);
            held.extend(fields::tag(fields::hold_rest(p, &covered), name));
        }
        merged.extend(held);
        let mut out = merge(merged);
        out.retain(|d| {
            let printed = d.field.is_none()
                && instructions.contains(&d.page)
                && matches!(
                    d.kind,
                    TokenKind::Person | TokenKind::Org | TokenKind::Address
                );
            !printed && !self.is_public_contact(extracted, &field_pages, d)
        });

        // A number placed once is the same kind wherever it repeats.
        let repeats = same_value::find(&extracted.pages, &field_pages, &out);
        if !repeats.is_empty() {
            out.extend(repeats);
            out = merge(out);
        }

        // Roles from the template field a detection sits in.
        for d in out
            .iter_mut()
            .filter(|d| d.role.is_none() && d.field.is_none())
        {
            if let Some(r) = from_template.regions.iter().find(|r| {
                r.page == d.page && r.span.start <= d.span.start && d.span.end <= r.span.end
            }) {
                d.role = Some(r.role.clone());
            }
        }

        unplaced::settle(&extracted.pages, &mut out);
        let class = classify::classify(extracted, matched.as_ref(), &out);
        if class.kind.as_str() == classify::BANK_STATEMENT {
            failclosed::settle_statement(&extracted.pages, &mut out);
        }
        tracing::debug!(
            kind = class.kind.as_str(),
            detections = out.len(),
            held = out.iter().filter(|d| d.needs_review).count(),
            "detected"
        );
        Ok((out, class))
    }

    /// A phone number or email address a pack lists as a public agency
    /// contact (an IRS hotline, phishing@irs.gov).
    fn is_public_contact(
        &self,
        extracted: &Extracted,
        field_pages: &[(&str, PageText)],
        d: &Detection,
    ) -> bool {
        if !matches!(d.kind, TokenKind::Phone | TokenKind::Email) {
            return false;
        }
        let text = match d.field.as_deref() {
            Some(name) => field_pages
                .iter()
                .find(|(n, _)| *n == name)
                .map(|(_, p)| p.text.as_str()),
            None => extracted
                .pages
                .iter()
                .find(|p| p.page == d.page)
                .map(|p| p.text.as_str()),
        };
        let Some(value) = text.and_then(|t| t.get(d.span.start..d.span.end)) else {
            return false;
        };
        match d.kind {
            TokenKind::Phone => self.packs.is_public_phone(value),
            _ => self.packs.is_public_email(value),
        }
    }
}

/// Resolve overlaps on each page (and within each form field): Template >
/// Rule > Model > FailClosed, then the longer span, then higher confidence.
///
/// A detection wholly inside a better one is dropped. One that reaches past a
/// better one is never dropped: against a `Keep` winner its uncovered parts
/// survive; otherwise the two become one tokenized detection over the union,
/// with the better one's kind, role and source.
/// The result is sorted by page and start; form-field detections come last.
pub fn merge(mut detections: Vec<Detection>) -> Vec<Detection> {
    detections.sort_by(better);
    let mut kept: Vec<Detection> = Vec::with_capacity(detections.len());
    for d in detections {
        let mut pending = vec![d];
        while let Some(d) = pending.pop() {
            let Some(i) = kept.iter().position(|k| {
                k.page == d.page
                    && k.field == d.field
                    && k.span.start < d.span.end
                    && d.span.start < k.span.end
            }) else {
                kept.push(d);
                continue;
            };
            let k = &kept[i];
            if k.span.start <= d.span.start && d.span.end <= k.span.end {
                continue;
            }
            if k.treatment == Treatment::Keep {
                if d.treatment != Treatment::Keep {
                    for (s, e) in [(d.span.start, k.span.start), (k.span.end, d.span.end)] {
                        if s < e {
                            let mut piece = d.clone();
                            piece.span = inker_core::Span { start: s, end: e };
                            pending.push(piece);
                        }
                    }
                }
                continue;
            }
            if d.treatment == Treatment::Keep {
                continue;
            }
            let k = kept.remove(i);
            pending.push(union(k, d));
        }
    }
    kept.sort_by(|a, b| {
        (a.field.is_some(), a.page, a.span.start)
            .cmp(&(b.field.is_some(), b.page, b.span.start))
            .then(a.field.cmp(&b.field))
    });
    kept
}

fn rank(s: &DetectionSource) -> u8 {
    match s {
        DetectionSource::Template { .. } => 0,
        DetectionSource::Rule { .. } => 1,
        DetectionSource::Model { .. } => 2,
        DetectionSource::FailClosed { .. } => 3,
    }
}

/// Merge order: the more precise source, then the longer span, then the
/// higher confidence.
fn better(a: &Detection, b: &Detection) -> std::cmp::Ordering {
    rank(&a.source)
        .cmp(&rank(&b.source))
        .then(b.span.len().cmp(&a.span.len()))
        .then(b.confidence.total_cmp(&a.confidence))
        .then(a.span.start.cmp(&b.span.start))
}

/// One tokenized detection over both spans, described by the more precise.
fn union(a: Detection, b: Detection) -> Detection {
    let (mut win, lose) = if rank(&a.source) <= rank(&b.source) {
        (a, b)
    } else {
        (b, a)
    };
    win.span = inker_core::Span {
        start: win.span.start.min(lose.span.start),
        end: win.span.end.max(lose.span.end),
    };
    win.treatment = Treatment::Tokenize;
    win.generalized = None;
    if lose.needs_review && !win.needs_review {
        win.needs_review = true;
        win.review_prompt = lose.review_prompt;
    }
    win
}

/// Detect with the default pipeline (no model).
pub fn detect(extracted: &Extracted, packs: &PackSet) -> Result<(Vec<Detection>, Classification)> {
    Detector::new(packs).detect(extracted)
}

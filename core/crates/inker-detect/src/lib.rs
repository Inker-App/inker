//! inker-detect: finds sensitive values and classifies documents.
//!
//! Input is [`Extracted`](inker_core::Extracted); output is a list of
//! [`Detection`](inker_core::Detection)s plus a [`Classification`]. This crate
//! never mints tokens and never sees the vault. See docs/stack-decision.md
//! section 3.5 for the pipeline it implements.

pub mod address;
pub mod classify;
pub mod failclosed;
pub mod model;
pub mod names;
pub mod rules;
pub mod template;
mod util;

pub use util::detection;

pub use classify::Classification;
pub use model::{Model, NoModel};

use inker_core::{Detection, DetectionSource, Extracted, Result};
use inker_packs::PackSet;

/// The detection pipeline: templates, rules, names, addresses, the model (if
/// any), then fail-closed; overlaps merged, roles attached from template fields.
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

        let mut all = from_template.detections.clone();
        for page in &extracted.pages {
            all.extend(rules::find(page));
            all.extend(names::find(page));
            all.extend(address::find(page));
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
            merged.extend(failclosed::find(page, &covered));
        }
        let mut out = merge(merged);

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

        let class = classify::classify(extracted, matched.as_ref(), &out);
        tracing::debug!(
            kind = class.kind.as_str(),
            detections = out.len(),
            held = out.iter().filter(|d| d.needs_review).count(),
            "detected"
        );
        Ok((out, class))
    }
}

/// Resolve overlaps on each page (and within each form field): Template >
/// Rule > Model > FailClosed, then the longer span, then higher confidence.
/// The result is sorted by page and start; form-field detections come last.
pub fn merge(mut detections: Vec<Detection>) -> Vec<Detection> {
    fn rank(s: &DetectionSource) -> u8 {
        match s {
            DetectionSource::Template { .. } => 0,
            DetectionSource::Rule { .. } => 1,
            DetectionSource::Model { .. } => 2,
            DetectionSource::FailClosed { .. } => 3,
        }
    }
    detections.sort_by(|a, b| {
        rank(&a.source)
            .cmp(&rank(&b.source))
            .then(b.span.len().cmp(&a.span.len()))
            .then(b.confidence.total_cmp(&a.confidence))
            .then(a.span.start.cmp(&b.span.start))
    });
    let mut kept: Vec<Detection> = Vec::with_capacity(detections.len());
    for d in detections {
        let clash = kept.iter().any(|k| {
            k.page == d.page
                && k.field == d.field
                && k.span.start < d.span.end
                && d.span.start < k.span.end
        });
        if !clash {
            kept.push(d);
        }
    }
    kept.sort_by(|a, b| {
        (a.field.is_some(), a.page, a.span.start)
            .cmp(&(b.field.is_some(), b.page, b.span.start))
            .then(a.field.cmp(&b.field))
    });
    kept
}

/// Detect with the default pipeline (no model).
pub fn detect(extracted: &Extracted, packs: &PackSet) -> Result<(Vec<Detection>, Classification)> {
    Detector::new(packs).detect(extracted)
}

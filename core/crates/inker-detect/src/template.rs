//! The form template engine. Templates are TOML data in a pack
//! (`packs/tax/forms/*.toml`), parsed and validated by `inker-packs`.
//!
//! Matching is anchor-first: a template applies to a page when every anchor
//! regex matches it. Then each field's label regex is found in the page text
//! and the value after it is read according to the field's shape, and fillable
//! AcroForm fields are matched by name.

use std::sync::LazyLock;

use inker_core::{Detection, DetectionSource, Extracted, Span, TextOrigin, TokenKind, Treatment};
use inker_packs::{FieldKind, FieldSpec, FormTemplate, PackSet, ValueShape};
use regex::Regex;

use crate::address;
use crate::util::{ceil_char, detection, isolated, re};

/// The template chosen for a document.
#[derive(Debug, Clone)]
pub struct TemplateMatch<'a> {
    pub template: &'a FormTemplate,
    /// Pages where every anchor matched.
    pub pages: Vec<u32>,
    pub year: Option<u16>,
}

/// A stretch of page text that belongs to a field with a role.
#[derive(Debug, Clone, PartialEq)]
pub struct RoleRegion {
    pub page: u32,
    pub span: Span,
    pub role: String,
}

/// What applying a template produced.
#[derive(Debug, Clone, Default)]
pub struct TemplateOutput {
    pub detections: Vec<Detection>,
    /// Values of `keep` fields: later stages must not fail-close them.
    pub keep: Vec<(u32, Span)>,
    pub regions: Vec<RoleRegion>,
}

/// Confidence of a template value on a text-layer page and on an OCR page.
const CONF_TEXT: f32 = 0.95;
const CONF_OCR: f32 = 0.85;
/// How far after a label to look for a TIN, routing or account value.
const WINDOW: usize = 160;

static TIN: LazyLock<Regex> =
    LazyLock::new(|| re(r"\d{3}-?\d{2}-?\d{4}|\d{2}-\d{7}|[X*]{3}-?[X*]{2}-?\d{4}|[X*]{5}\d{4}"));
static DATE: LazyLock<Regex> =
    LazyLock::new(|| re(r"\d{1,2}[/-]\d{1,2}[/-]\d{4}|\d{4}-\d{2}-\d{2}"));
static ROUTING: LazyLock<Regex> = LazyLock::new(|| re(r"\d{9}"));
static ACCOUNT: LazyLock<Regex> = LazyLock::new(|| re(r"[A-Za-z0-9*][A-Za-z0-9*-]{3,}"));

/// The best template for a document, if any applies: the one whose anchors
/// all match the most pages, then the one with the most anchors.
pub fn match_template<'a>(extracted: &Extracted, packs: &'a PackSet) -> Option<TemplateMatch<'a>> {
    let mut best: Option<(usize, usize, TemplateMatch<'a>)> = None;
    for t in packs.forms() {
        let pages: Vec<u32> = extracted
            .pages
            .iter()
            .filter(|p| t.anchors.iter().all(|a| a.is_match(&p.text)))
            .map(|p| p.page)
            .collect();
        if pages.is_empty() {
            continue;
        }
        let score = (pages.len(), t.anchors.len());
        if best.as_ref().is_some_and(|(n, a, _)| (*n, *a) >= score) {
            continue;
        }
        let year = t.year.as_ref().and_then(|y| {
            extracted
                .pages
                .iter()
                .filter(|p| pages.contains(&p.page))
                .find_map(|p| y.captures(&p.text)?.get(1)?.as_str().parse::<u16>().ok())
                .filter(|y| (1990..=2100).contains(y))
        });
        best = Some((
            score.0,
            score.1,
            TemplateMatch {
                template: t,
                pages,
                year,
            },
        ));
    }
    best.map(|(_, _, m)| m)
}

/// Read every field of the matched template.
pub fn apply(extracted: &Extracted, m: &TemplateMatch<'_>) -> TemplateOutput {
    let mut out = TemplateOutput::default();
    let t = m.template;
    for page in extracted.pages.iter().filter(|p| m.pages.contains(&p.page)) {
        let text = page.text.as_str();
        let conf = if page.origin == TextOrigin::Ocr {
            CONF_OCR
        } else {
            CONF_TEXT
        };
        let mut labels: Vec<(usize, usize)> = t
            .fields
            .iter()
            .flat_map(|f| f.label.find_iter(text).map(|m| (m.start(), m.end())))
            .collect();
        labels.sort_unstable();
        let page_ctx = Page {
            text,
            template: t,
            labels: &labels,
        };
        for f in &t.fields {
            for lm in f.label.find_iter(text) {
                let Some((start, end)) = page_ctx.value(f, lm.end()) else {
                    continue;
                };
                let FieldKind::Token(kind) = &f.kind else {
                    out.keep.push((page.page, Span { start, end }));
                    continue;
                };
                if out
                    .detections
                    .iter()
                    .any(|d| d.page == page.page && d.span.start < end && start < d.span.end)
                {
                    continue;
                }
                let mut d = field_detection(t, f, kind, &text[start..end], conf);
                d.page = page.page;
                d.span = Span { start, end };
                if let Some(role) = &f.role {
                    out.regions.push(RoleRegion {
                        page: page.page,
                        span: Span {
                            start: lm.start(),
                            end,
                        },
                        role: role.clone(),
                    });
                }
                out.detections.push(d);
            }
        }
    }
    acroform(extracted, t, &mut out);
    out.detections
        .sort_by_key(|d| (d.field.is_some(), d.page, d.span.start));
    out
}

/// A detection for a field's value; page and span are set by the caller.
fn field_detection(
    t: &FormTemplate,
    f: &FieldSpec,
    kind: &TokenKind,
    value: &str,
    conf: f32,
) -> Detection {
    let source = DetectionSource::Template {
        form: t.id.clone(),
        field: f.id.clone(),
    };
    let mut treatment = f.treatment;
    let mut generalized = None;
    if treatment == Treatment::Generalize {
        generalized = address::generalize(value);
        if generalized.is_none() {
            // Could not find "City, ST": ink the whole thing instead.
            treatment = Treatment::Tokenize;
        }
    }
    let mut d = detection(0, 0, 0, kind.clone(), source, conf, treatment);
    d.generalized = generalized;
    d.role = f.role.clone();
    d
}

/// AcroForm fields, matched by name.
fn acroform(extracted: &Extracted, t: &FormTemplate, out: &mut TemplateOutput) {
    for ff in &extracted.fields {
        let trimmed = ff.value.trim();
        if trimmed.is_empty() {
            continue;
        }
        let full = ff.name.to_ascii_lowercase();
        let last = full.rsplit('.').next().unwrap_or(&full);
        let last = last.split('[').next().unwrap_or(last);
        let Some((f, kind)) = t.fields.iter().find_map(|f| match &f.kind {
            FieldKind::Token(k)
                if f.acroform.iter().any(|a| {
                    let a = a.to_ascii_lowercase();
                    a == full || a == last
                }) =>
            {
                Some((f, k))
            }
            _ => None,
        }) else {
            continue;
        };
        let start = ff.value.len() - ff.value.trim_start().len();
        let mut d = field_detection(t, f, kind, trimmed, CONF_TEXT);
        d.page = ff.page.unwrap_or(1);
        d.span = Span {
            start,
            end: start + trimmed.len(),
        };
        d.field = Some(ff.name.clone());
        out.detections.push(d);
    }
}

/// One page being read against a template.
struct Page<'a> {
    text: &'a str,
    template: &'a FormTemplate,
    /// Every label match on the page, sorted.
    labels: &'a [(usize, usize)],
}

impl Page<'_> {
    /// True when `s` holds any label of the template: it is not a value.
    fn is_label(&self, s: &str) -> bool {
        self.template.fields.iter().any(|f| f.label.is_match(s))
    }

    /// End of the search window after a label: the next label, or `WINDOW`.
    fn window_end(&self, from: usize) -> usize {
        let cap = ceil_char(self.text, from + WINDOW);
        self.labels
            .iter()
            .map(|&(s, _)| s)
            .find(|&s| s >= from)
            .map_or(cap, |s| s.min(cap))
    }

    /// The value of field `f` whose label ends at `from`.
    fn value(&self, f: &FieldSpec, from: usize) -> Option<(usize, usize)> {
        match f.value {
            ValueShape::Tin => self.first_in_window(&TIN, from, |_| true),
            ValueShape::Routing => self.first_in_window(&ROUTING, from, |_| true),
            ValueShape::Date => self.first_in_window(&DATE, from, |_| true),
            ValueShape::Account => self.first_in_window(&ACCOUNT, from, |s| {
                s.bytes().filter(u8::is_ascii_digit).count() >= 4
            }),
            ValueShape::Line => {
                let (s, e) = self.lines(from).nth(f.skip_lines)?;
                let v = &self.text[s..e];
                let wants_letters =
                    matches!(f.kind, FieldKind::Token(TokenKind::Person | TokenKind::Org));
                if self.is_label(v)
                    || (wants_letters && !v.bytes().any(|b| b.is_ascii_alphabetic()))
                {
                    return None;
                }
                Some((s, e))
            }
            ValueShape::Lines => {
                let taken: Vec<(usize, usize)> = self
                    .lines(from)
                    .skip(f.skip_lines)
                    .take(f.lines)
                    .take_while(|&(s, e)| !self.is_label(&self.text[s..e]))
                    .collect();
                Some((taken.first()?.0, taken.last()?.1))
            }
        }
    }

    fn first_in_window(
        &self,
        re: &Regex,
        from: usize,
        ok: impl Fn(&str) -> bool,
    ) -> Option<(usize, usize)> {
        let end = self.window_end(from);
        re.find_iter(&self.text[from..end])
            .map(|m| (from + m.start(), from + m.end()))
            .find(|&(s, e)| isolated(self.text, s, e) && ok(&self.text[s..e]))
    }

    /// Value lines after a label: the rest of the label's line if it holds
    /// anything, then each following non-empty line. Each is cut to its first
    /// column segment (three spaces or a tab end a column).
    fn lines(&self, from: usize) -> impl Iterator<Item = (usize, usize)> + '_ {
        let text = self.text;
        let mut pos = from;
        let mut first = true;
        std::iter::from_fn(move || {
            loop {
                if pos >= text.len() {
                    return None;
                }
                let line_end = text[pos..].find('\n').map_or(text.len(), |i| pos + i);
                let line = &text[pos..line_end];
                let skip = if first {
                    line.len()
                        - line
                            .trim_start_matches([' ', '\t', ':', '|', '.', '-'])
                            .len()
                } else {
                    line.len() - line.trim_start().len()
                };
                first = false;
                let s = pos + skip;
                pos = line_end + 1;
                let rest = &text[s..line_end];
                let seg_len = ["   ", "\t"]
                    .iter()
                    .filter_map(|b| rest.find(b))
                    .min()
                    .unwrap_or(rest.len());
                let seg = rest[..seg_len].trim_end();
                if !seg.is_empty() {
                    return Some((s, s + seg.len()));
                }
            }
        })
    }
}

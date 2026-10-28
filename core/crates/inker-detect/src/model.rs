//! The optional ML pass over free text.

use inker_core::{Detection, PageText, Result, TokenKind};

/// A named-entity model run over page text. Detections it returns should use
/// `DetectionSource::Model`.
pub trait Model: std::fmt::Debug + Send + Sync {
    fn detect(&self, pages: &[PageText]) -> Result<Vec<Detection>>;
}

/// The default: no model, no detections.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoModel;

impl Model for NoModel {
    fn detect(&self, pages: &[PageText]) -> Result<Vec<Detection>> {
        let _ = pages;
        Ok(Vec::new())
    }
}

/// The entity labels asked of a GLiNER-PII model.
pub const GLINER_LABELS: &[&str] = &[
    "ssn",
    "routing number",
    "bank account",
    "person",
    "organization",
    "address",
    "email",
    "phone",
];

/// Map a model label to a token kind; `None` for labels we do not ink.
pub fn map_label(label: &str) -> Option<TokenKind> {
    let l = label.trim().to_ascii_lowercase();
    Some(match l.as_str() {
        "ssn" | "social security number" => TokenKind::Ssn,
        "routing number" | "aba routing number" => TokenKind::Routing,
        "bank account" | "account number" | "bank account number" => TokenKind::Account,
        "person" | "name" => TokenKind::Person,
        "organization" | "organisation" | "company" => TokenKind::Org,
        "address" | "street address" => TokenKind::Address,
        "email" | "email address" => TokenKind::Email,
        "phone" | "phone number" => TokenKind::Phone,
        _ => return None,
    })
}

/// Split `text` into pieces of at most `max` bytes, at line ends where
/// possible, as `(byte offset, piece)`. Models have a token limit.
pub fn chunks(text: &str, max: usize) -> Vec<(usize, &str)> {
    let max = max.max(4);
    let mut out = Vec::new();
    let mut start = 0;
    while start < text.len() {
        let mut end = crate::util::floor_char(text, (start + max).min(text.len()));
        if end < text.len() {
            // Prefer the last line end inside the window.
            if let Some(nl) = text[start..end].rfind('\n') {
                end = start + nl + 1;
            }
        }
        if end == start {
            // One character wider than `max`: take it whole.
            end = crate::util::ceil_char(text, start + 1);
        }
        out.push((start, &text[start..end]));
        start = end;
    }
    out
}

#[cfg(feature = "gliner")]
pub use gliner_model::{GlinerConfig, GlinerModel};

/// GLiNER-PII through `gline-rs` on ONNX Runtime. Only built with the
/// `gliner` feature, so the default build never links ONNX Runtime.
#[cfg(feature = "gliner")]
mod gliner_model {
    use std::path::PathBuf;

    use gliner::model::GLiNER;
    use gliner::model::input::text::TextInput;
    use gliner::model::params::Parameters;
    use gliner::model::pipeline::span::SpanMode;
    use inker_core::{DetectionSource, Error, Treatment};
    use orp::params::RuntimeParameters;

    use super::*;
    use crate::address;
    use crate::util::detection;

    /// Where the model lives on disk (from config), and its score threshold.
    #[derive(Debug, Clone)]
    pub struct GlinerConfig {
        /// `tokenizer.json` that ships with the model.
        pub tokenizer: PathBuf,
        /// The ONNX file, e.g. `onnx/model.onnx` (uint8 or fp16).
        pub model: PathBuf,
        /// Minimum span probability, 0.0 to 1.0.
        pub threshold: f32,
    }

    /// Bytes of text per inference call; GLiNER truncates past its token limit.
    const CHUNK: usize = 1200;

    pub struct GlinerModel {
        inner: GLiNER<SpanMode>,
    }

    impl std::fmt::Debug for GlinerModel {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("GlinerModel").finish_non_exhaustive()
        }
    }

    fn err(e: impl std::fmt::Display) -> Error {
        Error::Detect(format!("gliner: {e}"))
    }

    impl GlinerModel {
        /// Load the tokenizer and ONNX model named in `config`.
        pub fn load(config: &GlinerConfig) -> Result<Self> {
            let params = Parameters::default().with_threshold(config.threshold);
            let inner = GLiNER::<SpanMode>::new(
                params,
                RuntimeParameters::default(),
                &config.tokenizer,
                &config.model,
            )
            .map_err(err)?;
            Ok(Self { inner })
        }
    }

    impl Model for GlinerModel {
        fn detect(&self, pages: &[PageText]) -> Result<Vec<Detection>> {
            let mut out = Vec::new();
            for page in pages {
                let pieces = chunks(&page.text, CHUNK);
                if pieces.is_empty() {
                    continue;
                }
                let texts: Vec<&str> = pieces.iter().map(|(_, s)| *s).collect();
                let input = TextInput::from_str(&texts, GLINER_LABELS).map_err(err)?;
                let result = self.inner.inference(input).map_err(err)?;
                for (i, spans) in result.spans.iter().enumerate() {
                    let Some(&(offset, piece)) = pieces.get(i) else {
                        continue;
                    };
                    for span in spans {
                        let Some(kind) = map_label(span.class()) else {
                            continue;
                        };
                        let (s, e) = span.offsets();
                        if e > piece.len() || s >= e {
                            continue;
                        }
                        let source = DetectionSource::Model {
                            name: "gliner".into(),
                        };
                        let mut d = detection(
                            page.page,
                            offset + s,
                            offset + e,
                            kind.clone(),
                            source,
                            span.probability(),
                            Treatment::Tokenize,
                        );
                        if kind == TokenKind::Address
                            && let Some(g) = address::generalize(&piece[s..e])
                        {
                            d.treatment = Treatment::Generalize;
                            d.generalized = Some(g);
                        }
                        out.push(d);
                    }
                }
            }
            Ok(out)
        }
    }
}

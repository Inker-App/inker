//! inker-pipeline: drives documents through ingest, extract, detect and
//! tokenize. The shell calls this on a background thread; the CLI calls it
//! directly. See docs/stack-decision.md section 3.5.
//!
//! The pipeline never sees a real value on purpose: extraction returns text,
//! detection returns spans, and only `inker_core` mints tokens and stores
//! anything. Nothing here logs document content.

use camino::Utf8PathBuf;
use inker_core::{Core, DocState, DocumentId, IngestMode, Result};
use inker_detect::{Detector, Model};
use inker_extract::{Extractor, FileKind, detect_file_kind};
use inker_packs::PackSet;

/// What happened to one dropped path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    pub id: DocumentId,
    pub state: DocState,
    /// Why it failed, in words safe to show. Never contains document content.
    pub error: Option<String>,
}

/// Ingest, extract, detect, tokenize.
pub struct Pipeline {
    core: Core,
    extractor: Extractor,
    packs: PackSet,
    model: Option<Box<dyn Model>>,
}

impl std::fmt::Debug for Pipeline {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Pipeline")
            .field("model", &self.model.is_some())
            .finish_non_exhaustive()
    }
}

/// Lets the long-lived `Pipeline` lend its model to a per-document `Detector`.
#[derive(Debug)]
struct Borrowed<'a>(&'a dyn Model);

impl Model for Borrowed<'_> {
    fn detect(&self, pages: &[inker_core::PageText]) -> Result<Vec<inker_core::Detection>> {
        self.0.detect(pages)
    }
}

impl Pipeline {
    pub fn new(core: Core, extractor: Extractor, packs: PackSet) -> Self {
        Self {
            core,
            extractor,
            packs,
            model: None,
        }
    }

    /// Add an ML pass for free text. Off by default.
    pub fn with_model(mut self, model: Box<dyn Model>) -> Self {
        self.model = Some(model);
        self
    }

    pub fn core(&self) -> &Core {
        &self.core
    }

    /// Move or copy the paths into the private folder, then process each one
    /// in order. Files that cannot be processed are still moved and end up
    /// `Failed`. Returns one outcome per created document.
    pub fn ingest(&self, paths: &[Utf8PathBuf], mode: IngestMode) -> Result<Vec<Outcome>> {
        let ids = self.core.begin_ingest(paths, mode)?;
        Ok(ids.into_iter().map(|id| self.process(id)).collect())
    }

    /// Run extraction and detection for one queued document. Infallible from
    /// the caller's side: errors become a `Failed` record with a safe note.
    pub fn process(&self, id: DocumentId) -> Outcome {
        match self.run(id) {
            Ok(state) => Outcome {
                id,
                state,
                error: None,
            },
            Err(inker_core::Error::Refused(why)) => {
                let state = self
                    .core
                    .document(id)
                    .map(|r| r.state)
                    .unwrap_or(DocState::Failed);
                Outcome {
                    id,
                    state,
                    error: Some(why),
                }
            }
            Err(e) => {
                // Log the kind of failure only; the message can carry a path.
                tracing::warn!(document = ?id, kind = error_kind(&e), "processing failed");
                let note = "could not read this file";
                if let Err(e2) = self.core.set_state(id, DocState::Failed, Some(note)) {
                    tracing::error!(document = ?id, kind = error_kind(&e2), "could not mark failed");
                }
                Outcome {
                    id,
                    state: DocState::Failed,
                    error: Some(e.to_string()),
                }
            }
        }
    }

    fn run(&self, id: DocumentId) -> Result<DocState> {
        let rec = self.core.document(id)?;
        if rec.kind.as_str() == inker_core::facade::OUTPUT_KIND {
            // An output is rehydrated on disk. Re-extracting it would hand
            // the agent whatever detection misses. Leave it exactly as is.
            return Err(inker_core::Error::Refused(
                "agent outputs are not processed".into(),
            ));
        }
        let note = match detect_file_kind(&rec.path) {
            FileKind::Image => "reading the scan",
            _ => "reading",
        };
        self.core.set_state(id, DocState::Reading, Some(note))?;
        let extracted = self.extractor.extract(&rec.path)?;

        self.core.set_state(id, DocState::Inking, Some("inking"))?;
        let mut detector = Detector::new(&self.packs);
        if let Some(model) = self.model.as_deref() {
            detector = detector.with_model(Box::new(Borrowed(model)));
        }
        let (detections, class) = detector.detect(&extracted)?;
        tracing::info!(document = ?id, kind = %class.kind.as_str(), found = detections.len(), "detected");

        self.core
            .complete_processing(id, &extracted, &detections, class.kind, class.year)
    }
}

/// A short, content-free label for logs.
fn error_kind(e: &inker_core::Error) -> &'static str {
    use inker_core::Error as E;
    match e {
        E::InvalidHandle(_) => "invalid_handle",
        E::UnknownHandle(_) => "unknown_handle",
        E::PendingReview(_) => "pending_review",
        E::NotReady(_) => "not_ready",
        E::Vault(_) => "vault",
        E::Index(_) => "index",
        E::Extract(_) => "extract",
        E::Detect(_) => "detect",
        E::Ingest(_) => "ingest",
        E::Refused(_) => "refused",
        E::Io(_) => "io",
        E::Json(_) => "json",
    }
}

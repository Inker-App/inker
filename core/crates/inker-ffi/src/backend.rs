//! The seam between the FFI façade and whatever does the work.

use std::fmt;
use std::sync::Arc;

use camino::Utf8PathBuf;

use crate::error::FfiError;
use crate::types::{QueueItem, ReadEvent, ReviewAnswer, Status};

/// Called by a backend whenever the queue changes. Cheap and thread-safe; the
/// handle forwards it to the Swift listener.
#[derive(Clone)]
pub struct ChangeSink(Arc<dyn Fn() + Send + Sync>);

impl ChangeSink {
    pub fn new(f: impl Fn() + Send + Sync + 'static) -> Self {
        Self(Arc::new(f))
    }

    pub fn noop() -> Self {
        Self::new(|| {})
    }

    pub fn notify(&self) {
        (self.0)()
    }
}

impl fmt::Debug for ChangeSink {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ChangeSink")
    }
}

/// What the shell needs from the core. `HostBackend` is the real one;
/// `PreviewBackend` fakes it for SwiftUI previews and tests.
///
/// Implementations must be callable from any thread and must never put a real
/// sensitive value into anything they return.
pub trait Backend: Send + Sync + fmt::Debug {
    /// Install the sink to call on every queue change. Replaces any earlier one.
    fn set_change_sink(&self, sink: ChangeSink);
    /// Start ingesting dropped files or folders. Returns once queued; progress
    /// is reported through the change sink.
    fn ingest(
        &self,
        paths: Vec<Utf8PathBuf>,
        mode: inker_core::types::IngestMode,
    ) -> Result<(), FfiError>;
    /// The whole queue, newest drop first.
    fn queue(&self) -> Vec<QueueItem>;
    /// Answer the attention card.
    fn answer(&self, question_id: &str, answer: ReviewAnswer) -> Result<(), FfiError>;
    /// The most recent agent reads, newest first, at most `n`.
    fn recent_reads(&self, n: u32) -> Vec<ReadEvent>;
    /// Footer state.
    fn status(&self) -> Status;
    /// Stop background work (worker threads, the socket). Idempotent. The
    /// default does nothing.
    fn close(&self) {}
}

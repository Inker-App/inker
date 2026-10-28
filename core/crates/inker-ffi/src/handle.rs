//! `CoreHandle`, the one object Swift holds.

use std::fmt;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use camino::Utf8PathBuf;
use inker_core::types::{Codename, Handle};

use crate::agents::{self, AgentKind, DoctorReport, Plan};
use crate::backend::{Backend, ChangeSink};
use crate::error::FfiError;
use crate::preview::{PreviewBackend, PreviewScenario};
use crate::types::{HandleStyle, IngestMode, QueueItem, ReadEvent, ReviewAnswer, Status};

/// Implemented in Swift. Fired on any queue change, from a background thread;
/// the Swift side hops to the main actor before touching UI.
///
/// Note: UniFFI 0.32 soft-deprecates `callback_interface` in favour of
/// `#[uniffi::export(foreign)]` with `Arc<dyn Trait>`. It still works and is
/// what the generated Swift protocol expects today.
#[uniffi::export(callback_interface)]
pub trait QueueListener: Send + Sync {
    fn queue_changed(&self);
}

type ListenerSlot = Arc<Mutex<Option<Box<dyn QueueListener>>>>;

/// Simulated pipeline step for the live preview backend `open` returns.
const LIVE_TICK: Duration = Duration::from_millis(700);

#[derive(uniffi::Object)]
pub struct CoreHandle {
    backend: Arc<dyn Backend>,
    listener: ListenerSlot,
}

impl fmt::Debug for CoreHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CoreHandle")
            .field("backend", &self.backend)
            .finish_non_exhaustive()
    }
}

impl CoreHandle {
    /// Wrap any backend. Rust-only: this is where the real core gets wired.
    pub fn with_backend(backend: Arc<dyn Backend>) -> Arc<Self> {
        let listener: ListenerSlot = Arc::new(Mutex::new(None));
        let slot = listener.clone();
        backend.set_change_sink(ChangeSink::new(move || {
            // Hold the lock while calling so clear_listener cannot race a call
            // into a listener Swift is about to drop. Swift must not call
            // set_listener/clear_listener from inside queue_changed.
            if let Ok(guard) = slot.lock()
                && let Some(l) = guard.as_ref()
            {
                l.queue_changed();
            }
        }));
        Arc::new(Self { backend, listener })
    }
}

#[uniffi::export]
impl CoreHandle {
    /// Open the core. Until the real core is wired this returns the live
    /// preview backend: drops are simulated and files are never touched.
    #[uniffi::constructor]
    pub fn open(private_folder: String, data_dir: String) -> Result<Arc<Self>, FfiError> {
        if private_folder.trim().is_empty() {
            return Err(FfiError::InvalidArgument {
                reason: "private folder is empty".into(),
            });
        }
        tracing::debug!(
            data_dir_set = !data_dir.is_empty(),
            "opening preview backend"
        );
        let display = display_path(&private_folder);
        Ok(Self::with_backend(Arc::new(PreviewBackend::live(
            &display, LIVE_TICK,
        ))))
    }

    /// A handle frozen in one of the four popover states, for SwiftUI previews.
    #[uniffi::constructor]
    pub fn preview(scenario: PreviewScenario) -> Arc<Self> {
        Self::with_backend(Arc::new(PreviewBackend::scenario(scenario)))
    }

    /// Ingest dropped files or folders.
    pub fn ingest(&self, paths: Vec<String>, mode: IngestMode) -> Result<(), FfiError> {
        let paths = paths.into_iter().map(Utf8PathBuf::from).collect();
        self.backend.ingest(paths, mode.into())
    }

    pub fn queue(&self) -> Vec<QueueItem> {
        self.backend.queue()
    }

    pub fn answer(&self, question_id: String, answer: ReviewAnswer) -> Result<(), FfiError> {
        self.backend.answer(&question_id, answer)
    }

    /// `🐙/w2-acme-2025` or `ink/w2-acme-2025`, for the clipboard.
    pub fn handle_string(&self, codename: String, style: HandleStyle) -> Result<String, FfiError> {
        let codename =
            Codename::parse(&codename).map_err(|reason| FfiError::InvalidArgument { reason })?;
        Ok(Handle::new(codename).render(style.into()))
    }

    pub fn recent_reads(&self, n: u32) -> Vec<ReadEvent> {
        self.backend.recent_reads(n)
    }

    /// Replace the queue listener.
    pub fn set_listener(&self, listener: Box<dyn QueueListener>) {
        if let Ok(mut slot) = self.listener.lock() {
            *slot = Some(listener);
        }
    }

    pub fn clear_listener(&self) {
        if let Ok(mut slot) = self.listener.lock() {
            *slot = None;
        }
    }

    /// Stub: fixed plans; refuses to write (see `agents`).
    pub fn setup_agent(&self, agent: AgentKind, dry_run: bool) -> Result<Plan, FfiError> {
        agents::setup_agent(agent, dry_run)
    }

    /// Stub: fixed text.
    pub fn doctor(&self) -> DoctorReport {
        agents::doctor()
    }

    pub fn status(&self) -> Status {
        self.backend.status()
    }
}

/// `/Users/x/Private` -> `~/Private` when it is under `$HOME`.
fn display_path(path: &str) -> String {
    if let Ok(home) = std::env::var("HOME") {
        let home = home.trim_end_matches('/');
        if !home.is_empty()
            && let Some(rest) = path.strip_prefix(home)
            && (rest.is_empty() || rest.starts_with('/'))
        {
            return format!("~{rest}");
        }
    }
    path.to_string()
}

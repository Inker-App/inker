//! `CoreHandle`, the one object Swift holds.

use std::fmt;
use std::sync::{Arc, Mutex};

use camino::Utf8PathBuf;
use inker_core::types::{Codename, Handle};
use inker_extract::Extractor;
use inker_host::{Host, HostConfig};

use crate::agents::{self, AgentKind, AgentReport, DoctorReport, Plan};
use crate::backend::{Backend, ChangeSink};
use crate::error::FfiError;
use crate::host_backend::{HostBackend, core_error};
use crate::preview::{PreviewBackend, PreviewScenario};
use crate::providers::{
    HostProviders, ProviderKeyStore, ProviderOcrSource, ProviderTextSource, SharedProviders,
};
use crate::types::{
    Board, BoardEntry, DocDetails, FindMatch, HandleStyle, IngestMode, QueueItem, ReadEvent,
    ReviewAnswer, Status,
};

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

    /// Open the real core with the pure-Rust providers: the file key store in
    /// the data dir, `pdf-extract` for text layers and fixture OCR. For Linux,
    /// the CLI's tests and this crate's tests. Starts the socket like `open`.
    pub fn open_with_rust_providers(
        private_folder: String,
        data_dir: String,
    ) -> Result<Arc<Self>, FfiError> {
        Self::open_host(private_folder, data_dir, Host::open)
    }

    fn open_host(
        private_folder: String,
        data_dir: String,
        open: impl FnOnce(HostConfig) -> inker_core::Result<Host>,
    ) -> Result<Arc<Self>, FfiError> {
        if private_folder.trim().is_empty() {
            return Err(FfiError::InvalidArgument {
                reason: "private folder is empty".into(),
            });
        }
        if data_dir.trim().is_empty() {
            return Err(FfiError::InvalidArgument {
                reason: "data dir is empty".into(),
            });
        }
        let config = HostConfig {
            private_folder: Utf8PathBuf::from(private_folder.as_str()),
            data_dir: Utf8PathBuf::from(data_dir.as_str()),
            packs_dir: None,
        };
        let socket = config.data_dir.join(inker_ipc::paths::SOCKET_NAME);
        let host = open(config).map_err(|e| core_error(&e))?;
        let home = std::env::var("HOME").ok();
        let backend = HostBackend::new(host, &display_path(&private_folder, home.as_deref()))?;
        if let Err(e) = backend.serve_socket(socket.into_std_path_buf()) {
            tracing::warn!(error = %e, "MCP socket not started");
        }
        Ok(Self::with_backend(backend))
    }
}

#[uniffi::export]
impl CoreHandle {
    /// Open the real core: vault, index and key in `data_dir`, documents in
    /// `private_folder`, and the MCP socket at `<data_dir>/inker.sock`. The
    /// key, PDF text and OCR come from `providers` (Keychain, PDFKit, Vision).
    /// Drops then really move or copy files.
    #[uniffi::constructor]
    pub fn open(
        private_folder: String,
        data_dir: String,
        providers: Box<dyn HostProviders>,
    ) -> Result<Arc<Self>, FfiError> {
        let providers: SharedProviders = Arc::from(providers);
        let extractor = Extractor::new(
            ProviderTextSource(providers.clone()),
            ProviderOcrSource(providers.clone()),
        );
        let keystore = ProviderKeyStore(providers);
        Self::open_host(private_folder, data_dir, |config| {
            Host::open_with_keystore(config, extractor, &keystore)
        })
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

    /// Plan (dry run) or apply one agent's configuration with the writers
    /// in `inker-agents`. `inker_bin` is the CLI inside the app bundle;
    /// `home` the user's home folder. Refused in a preview.
    pub fn setup_agent(
        &self,
        agent: AgentKind,
        dry_run: bool,
        inker_bin: String,
        home: String,
    ) -> Result<Plan, FfiError> {
        let private = self
            .backend
            .private_folder()
            .ok_or_else(|| FfiError::Backend {
                reason: "no private folder to set agents up for in a preview".into(),
            })?;
        agents::setup_agent(agent, dry_run, &inker_bin, &home, &private)
    }

    /// Whether an agent is installed and has INKER in its config. A preview
    /// reports both true, so its Settings screen reads "Connected".
    pub fn agent_report(&self, agent: AgentKind, inker_bin: String, home: String) -> AgentReport {
        match self.backend.private_folder() {
            Some(private) => agents::agent_report(agent, &inker_bin, &home, &private),
            None => AgentReport {
                installed: true,
                registered: true,
            },
        }
    }

    /// Whether the MCP socket is live, the data dir and the private folder.
    pub fn doctor(&self) -> DoctorReport {
        DoctorReport {
            lines: self.backend.doctor(),
        }
    }

    pub fn status(&self) -> Status {
        self.backend.status()
    }

    /// The ready board grouped by year. `tag` keeps only the documents under
    /// that chip; the chip counts and the total always cover everything.
    pub fn board(&self, tag: Option<String>) -> Board {
        let board = self.backend.board();
        match tag {
            Some(t) => board.filtered(&t),
            None => board,
        }
    }

    /// Find: best field first, then newest drop. The handle prefix is
    /// optional. Takes the core's lock on the real backend, so call it off
    /// the main thread.
    pub fn find(&self, query: String) -> Vec<FindMatch> {
        self.backend.find(&query)
    }

    /// The document or folder a pasted handle (`🐙/x` or `ink/x`, nothing
    /// else) points at, if the app holds it. For the clipboard chip and the
    /// footer's last-read button.
    pub fn locate(&self, handle: String) -> Option<BoardEntry> {
        let h = Handle::parse(&handle).ok()?;
        self.backend.locate(h.codename.as_str())
    }

    /// The expanded row.
    pub fn details(&self, id: String) -> Result<DocDetails, FfiError> {
        self.backend.details(&id)
    }

    /// "Forget this file" or "Forget this folder": out of the index and out
    /// of every write's scope. The files stay where they are.
    pub fn forget(&self, id: String) -> Result<(), FfiError> {
        self.backend.forget(&id)
    }

    /// "Ink again": read and ink the original again, keeping the handle.
    pub fn ink_again(&self, id: String) -> Result<(), FfiError> {
        self.backend.ink_again(&id)
    }

    /// Stop the MCP socket and the ingest worker. Call on quit; dropping the
    /// handle does the same. Unprocessed drops stay queued for the next open.
    pub fn close(&self) {
        self.backend.close();
    }
}

/// `/Users/x/Private` -> `~/Private` when it is under `home`.
fn display_path(path: &str, home: Option<&str>) -> String {
    if let Some(home) = home {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_path_shortens_home_to_a_tilde() {
        assert_eq!(
            display_path("/Users/x/Private", Some("/Users/x")),
            "~/Private"
        );
        assert_eq!(
            display_path("/Users/x/Private", Some("/Users/x/")),
            "~/Private"
        );
        assert_eq!(display_path("/Users/x", Some("/Users/x")), "~");
        assert_eq!(
            display_path("/Users/xy/Private", Some("/Users/x")),
            "/Users/xy/Private"
        );
        assert_eq!(display_path("/Volumes/P", Some("/Users/x")), "/Volumes/P");
        assert_eq!(display_path("/Users/x/P", None), "/Users/x/P");
        assert_eq!(display_path("/Users/x/P", Some("")), "/Users/x/P");
    }
}

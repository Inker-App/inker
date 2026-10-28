//! inker-host: the composition root. Opens the core, builds the pipeline, and
//! adapts it to the MCP [`Backend`] trait. The CLI and the Swift shell both
//! build on this so there is exactly one place that decides how the pieces
//! fit (docs/stack-decision.md section 3.2).
//!
//! One process, one `Host`, one lock: the core is behind a mutex so the MCP
//! server (many connections) and the shell (drops, answers) serialise on it.
//! Processing a document holds the lock for its duration; that is seconds,
//! and keeps token minting strictly ordered.

use std::fs;
use std::ops::Deref;
use std::sync::{Mutex, MutexGuard};

use camino::Utf8PathBuf;
use inker_core::ports::{HostPaths, KeyStore};
use inker_core::{AgentRead, Core, Description, DocumentId, Error, Handle, IngestMode, ListEntry, Result};
use inker_extract::{Extractor, FixtureOcr, PdfTextSource};
use inker_mcp::{Backend, Resolved, WriteReceipt};
use inker_packs::PackSet;
use inker_pipeline::{Outcome, Pipeline};

/// Where the host keeps things.
#[derive(Debug, Clone)]
pub struct HostConfig {
    /// The user's private folder, chosen at onboarding.
    pub private_folder: Utf8PathBuf,
    /// Vault, index, key and socket live here.
    pub data_dir: Utf8PathBuf,
    /// Override the built-in tax pack with a directory of packs.
    pub packs_dir: Option<Utf8PathBuf>,
}

/// A 32-byte key kept in a 0600 file. This is the Linux and CLI key store;
/// on macOS the shell supplies a Keychain-backed [`KeyStore`] instead.
#[derive(Debug, Clone)]
pub struct FileKeyStore {
    dir: Utf8PathBuf,
}

impl FileKeyStore {
    pub const FILE_NAME: &'static str = "vault.key";

    pub fn new(dir: Utf8PathBuf) -> Self {
        Self { dir }
    }

    fn path(&self) -> Utf8PathBuf {
        self.dir.join(Self::FILE_NAME)
    }
}

impl KeyStore for FileKeyStore {
    fn load_or_create(&self) -> Result<[u8; 32]> {
        let path = self.path();
        if path.exists() {
            let bytes = fs::read(&path)?;
            let key: [u8; 32] = bytes
                .as_slice()
                .try_into()
                .map_err(|_| Error::Vault("key file has the wrong length".into()))?;
            return Ok(key);
        }
        let mut key = [0u8; 32];
        getrandom::fill(&mut key).map_err(|e| Error::Vault(format!("random key: {e}")))?;
        fs::create_dir_all(&self.dir)?;
        let mut opts = fs::OpenOptions::new();
        opts.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        use std::io::Write;
        let mut f = opts.open(&path)?;
        f.write_all(&key)?;
        f.sync_all()?;
        Ok(key)
    }
}

/// The running core: pipeline behind a lock, exposed as an MCP backend.
pub struct Host {
    pipeline: Mutex<Pipeline>,
}

impl std::fmt::Debug for Host {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Host").finish_non_exhaustive()
    }
}

/// Read access to the core while the host lock is held.
#[derive(Debug)]
pub struct CoreGuard<'a>(MutexGuard<'a, Pipeline>);

impl Deref for CoreGuard<'_> {
    type Target = Core;
    fn deref(&self) -> &Core {
        self.0.core()
    }
}

impl Host {
    /// Open with the pure-Rust extractor (PDF text layer plus fixture OCR).
    /// The macOS shell uses [`Host::open_with`] and passes Vision and PDFKit.
    pub fn open(config: HostConfig) -> Result<Self> {
        let extractor = Extractor::new(PdfTextSource::new(), FixtureOcr::new());
        Self::open_with(config, extractor)
    }

    /// Open with a caller-supplied extractor and the file key store.
    pub fn open_with(config: HostConfig, extractor: Extractor) -> Result<Self> {
        let keystore = FileKeyStore::new(config.data_dir.clone());
        Self::open_with_keystore(config, extractor, &keystore)
    }

    /// Open with everything injected. This is what the Swift shell calls.
    pub fn open_with_keystore(
        config: HostConfig,
        extractor: Extractor,
        keystore: &dyn KeyStore,
    ) -> Result<Self> {
        fs::create_dir_all(&config.private_folder)?;
        fs::create_dir_all(&config.data_dir)?;
        let core = Core::open(
            HostPaths {
                private_folder: config.private_folder.clone(),
                data_dir: config.data_dir.clone(),
            },
            keystore,
        )?;
        let packs = PackSet::load(config.packs_dir.as_deref().map(|p| p.as_std_path()))
            .map_err(|e| Error::Detect(format!("packs: {e}")))?;
        let pipeline = Pipeline::new(core, extractor, packs);
        tracing::info!(private = %config.private_folder, data = %config.data_dir, "host open");
        Ok(Self { pipeline: Mutex::new(pipeline) })
    }

    fn lock(&self) -> MutexGuard<'_, Pipeline> {
        self.pipeline.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// The core, for the shell's queue and review calls.
    pub fn core(&self) -> CoreGuard<'_> {
        CoreGuard(self.lock())
    }

    /// Move or copy files in and process them. Holds the lock throughout.
    pub fn ingest(&self, paths: &[Utf8PathBuf], mode: IngestMode) -> Result<Vec<Outcome>> {
        self.lock().ingest(paths, mode)
    }

    /// Re-run processing for one document (after an answer, or a retry).
    pub fn process(&self, id: DocumentId) -> Outcome {
        self.lock().process(id)
    }

    pub fn recent_reads(&self, n: usize) -> Result<Vec<AgentRead>> {
        self.lock().core().recent_reads(n)
    }

    /// This host as the MCP backend.
    pub fn backend(&self) -> &dyn Backend {
        self
    }
}

impl Backend for Host {
    fn list(&self) -> Result<Vec<ListEntry>> {
        self.lock().core().list()
    }

    fn resolve(&self, h: &Handle) -> Result<Resolved> {
        Ok(match self.lock().core().resolve(h)? {
            inker_core::Resolved::Document(d) => Resolved::Document(d),
            inker_core::Resolved::Folder(f) => Resolved::Folder(f),
        })
    }

    fn describe(&self, h: &Handle) -> Result<Description> {
        self.lock().core().describe(h)
    }

    fn write(&self, h: &Handle, name: &str, content: &str) -> Result<WriteReceipt> {
        let out = self.lock().core().write(h, name, content)?;
        Ok(WriteReceipt { path: out.path, handle: out.handle })
    }

    fn note_read(&self, agent: &str, h: &Handle) {
        if let Err(e) = self.lock().core().record_agent_read(agent, h) {
            tracing::warn!(error = %e, "could not record agent read");
        }
    }
}

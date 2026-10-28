#![allow(clippy::expect_used, clippy::unwrap_used)]
//! The host opens a real core on disk, ingests through the pipeline, and
//! serves it to the MCP layer through the `Backend` trait. These tests prove
//! the adapter never hands the MCP layer a real value and that the key
//! survives a reopen.

use std::fs;

use camino::{Utf8Path, Utf8PathBuf};
use inker_core::ports::KeyStore;
use inker_core::{DocState, Error, Handle, IngestMode};
use inker_host::{FileKeyStore, Host, HostConfig};
use inker_mcp::{Backend, Resolved};

const SAMPLES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../samples/synthetic");
const JORDAN_SSN: &str = "900-12-3456";

struct World {
    _tmp: tempfile::TempDir,
    downloads: Utf8PathBuf,
    config: HostConfig,
}

impl World {
    fn new() -> Self {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tmp.path().to_path_buf()).expect("utf8");
        let downloads = root.join("Downloads");
        fs::create_dir_all(&downloads).expect("mkdir");
        let config = HostConfig {
            private_folder: root.join("Private"),
            data_dir: root.join("data"),
            packs_dir: None,
        };
        Self { _tmp: tmp, downloads, config }
    }

    fn open(&self) -> Host {
        Host::open(self.config.clone()).expect("open host")
    }

    fn download(&self, name: &str) -> Utf8PathBuf {
        let src = Utf8Path::new(SAMPLES).join(name);
        let dst = self.downloads.join(name);
        fs::copy(&src, &dst).unwrap_or_else(|e| panic!("copy {src}: {e}"));
        dst
    }
}

#[test]
fn open_creates_the_folders_and_the_key_with_private_permissions() {
    let w = World::new();
    let _host = w.open();
    assert!(w.config.private_folder.is_dir());
    assert!(w.config.data_dir.is_dir());
    let key = w.config.data_dir.join("vault.key");
    assert!(key.is_file(), "key file should exist after open");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(&key).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "key file mode {mode:o}");
    }
}

#[test]
fn file_keystore_is_stable_across_opens_and_random_across_dirs() {
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let pa = Utf8PathBuf::from_path_buf(a.path().join("data")).unwrap();
    let pb = Utf8PathBuf::from_path_buf(b.path().join("data")).unwrap();
    fs::create_dir_all(&pa).unwrap();
    fs::create_dir_all(&pb).unwrap();
    let k1 = FileKeyStore::new(pa.clone()).load_or_create().unwrap();
    let k2 = FileKeyStore::new(pa).load_or_create().unwrap();
    let k3 = FileKeyStore::new(pb).load_or_create().unwrap();
    assert_eq!(k1, k2, "same dir, same key");
    assert_ne!(k1, k3, "different dir, different key");
    assert_ne!(k1, [0u8; 32], "key must not be all zeros");
}

#[test]
fn ingest_then_resolve_through_the_backend_shows_tokens_only() {
    let w = World::new();
    let host = w.open();
    let src = w.download("w2-acme-2025.txt");

    let outcomes = host.ingest(&[src], IngestMode::Move).expect("ingest");
    assert_eq!(outcomes[0].state, DocState::Ready);

    let backend: &dyn Backend = host.backend();
    let entries = backend.list().expect("list");
    assert_eq!(entries.len(), 1);
    let handle = entries[0].handle.clone();
    assert!(handle.codename.as_str().starts_with("w2-"));

    match backend.resolve(&handle).expect("resolve") {
        Resolved::Document(d) => {
            let text: String = d.pages.iter().map(|p| p.text.as_str()).collect();
            assert!(text.contains("[SSN_"));
            assert!(!text.contains(JORDAN_SSN));
            for f in &d.fields {
                assert!(!f.value.contains(JORDAN_SSN));
            }
        }
        Resolved::Folder(_) => panic!("expected a document"),
    }

    let desc = backend.describe(&handle).expect("describe");
    assert!(desc.roles.iter().any(|r| r.role == "employee"), "{:?}", desc.roles);

    backend.note_read("Claude Code", &handle);
    let reads = host.recent_reads(5).expect("reads");
    assert_eq!(reads.len(), 1);
    assert_eq!(reads[0].agent, "Claude Code");
}

#[test]
fn write_through_the_backend_lands_in_the_private_folder_only() {
    let w = World::new();
    let host = w.open();
    let src = w.download("w2-acme-2025.txt");
    host.ingest(&[src], IngestMode::Copy).expect("ingest");
    let handle = host.backend().list().unwrap()[0].handle.clone();

    let receipt = host
        .backend()
        .write(&handle, "summary.md", "Employee [SSN_1] earned 84,250.00 at [ORG_1].")
        .expect("write");
    assert!(receipt.path.starts_with(&w.config.private_folder), "{}", receipt.path);
    let saved = fs::read_to_string(&receipt.path).unwrap();
    assert!(saved.contains(JORDAN_SSN), "output on disk is rehydrated: {saved}");

    // The agent can only ever see the tokenized view of what it wrote.
    match host.backend().resolve(&receipt.handle).expect("resolve output") {
        Resolved::Document(d) => {
            let text: String = d.pages.iter().map(|p| p.text.as_str()).collect();
            assert!(text.contains("[SSN_1]"));
            assert!(!text.contains(JORDAN_SSN));
        }
        Resolved::Folder(_) => panic!("expected a document"),
    }

    let bad = host.backend().write(&handle, "../escape.md", "x").unwrap_err();
    assert!(matches!(bad, Error::Refused(_)), "{bad}");
}

#[test]
fn tokens_survive_a_reopen_of_the_same_data_dir() {
    let w = World::new();
    let first_token = {
        let host = w.open();
        let src = w.download("w2-acme-2025.txt");
        host.ingest(&[src], IngestMode::Move).unwrap();
        let h = host.backend().list().unwrap()[0].handle.clone();
        let Resolved::Document(d) = host.backend().resolve(&h).unwrap() else { panic!() };
        let text: String = d.pages.iter().map(|p| p.text.as_str()).collect();
        let i = text.find("[SSN_").unwrap();
        text[i..i + text[i..].find(']').unwrap() + 1].to_string()
    };
    let host = w.open();
    let src = w.download("k1-tidewater-2025.txt");
    host.ingest(&[src], IngestMode::Move).unwrap();
    let entries = host.backend().list().unwrap();
    let names: Vec<String> = entries.iter().map(|e| e.handle.to_string()).collect();
    assert_eq!(entries.len(), 2, "{names:?}");
    let k1 = entries
        .into_iter()
        .find(|e| !e.handle.codename.as_str().starts_with("w2-"))
        .unwrap_or_else(|| panic!("the K-1 should be listed beside the W-2: {names:?}"))
        .handle;
    assert!(k1.codename.as_str().contains("k1"), "K-1 codename should mention k1: {k1}");
    let Resolved::Document(d) = host.backend().resolve(&k1).unwrap() else { panic!() };
    let text: String = d.pages.iter().map(|p| p.text.as_str()).collect();
    assert!(text.contains(&first_token), "Jordan should still be {first_token} after reopen");
}

#[test]
fn a_held_document_is_pending_through_the_backend() {
    let w = World::new();
    let host = w.open();
    let src = w.downloads.join("note.txt");
    fs::write(&src, "Wire confirmation 00000000000 received\n").unwrap();
    let out = host.ingest(&[src], IngestMode::Move).unwrap();
    assert_eq!(out[0].state, DocState::NeedsAttention);
    let handle = Handle::new(host.core().document(out[0].id).unwrap().codename);
    let err = host.backend().resolve(&handle).unwrap_err();
    assert!(matches!(err, Error::PendingReview(_)));
}

#![allow(clippy::unwrap_used, clippy::expect_used)]
//! The Swift providers (Keychain, PDFKit, Vision) reach the core through the
//! `HostProviders` callback interface. A Rust fake stands in for Swift here.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use inker_ffi::{
    CoreHandle, DocItem, DocState, ExtractedFfi, ExtractedField, ExtractedPage, FfiError,
    HostProviders, IngestMode, ProviderError, QueueItem,
};

const SAMPLES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../samples/synthetic");
const JORDAN_SSN: &str = "900-12-3456";

#[derive(Debug, Default)]
struct Calls {
    key: usize,
    pdf_text: Vec<String>,
    ocr: Vec<String>,
}

/// What the fake hands back for each callback.
#[derive(Clone)]
struct Fake {
    key: Vec<u8>,
    text_layer: Option<ExtractedFfi>,
    ocr: Result<ExtractedFfi, String>,
    calls: Arc<Mutex<Calls>>,
}

impl Fake {
    fn new(text_layer: Option<ExtractedFfi>, ocr: Result<ExtractedFfi, String>) -> Self {
        Self {
            key: vec![7u8; 32],
            text_layer,
            ocr,
            calls: Arc::default(),
        }
    }
}

impl HostProviders for Fake {
    fn load_or_create_key(&self) -> Result<Vec<u8>, ProviderError> {
        self.calls.lock().unwrap().key += 1;
        Ok(self.key.clone())
    }

    fn pdf_text(&self, path: String) -> Result<Option<ExtractedFfi>, ProviderError> {
        self.calls.lock().unwrap().pdf_text.push(path);
        Ok(self.text_layer.clone())
    }

    fn ocr(&self, path: String) -> Result<ExtractedFfi, ProviderError> {
        self.calls.lock().unwrap().ocr.push(path);
        self.ocr
            .clone()
            .map_err(|reason| ProviderError::Failed { reason })
    }
}

fn w2_text() -> String {
    fs::read_to_string(Path::new(SAMPLES).join("w2-acme-2025.txt")).unwrap()
}

fn one_page(text: String) -> ExtractedFfi {
    ExtractedFfi {
        pages: vec![ExtractedPage { page: 1, text }],
        fields: vec![],
    }
}

struct World {
    tmp: tempfile::TempDir,
}

impl World {
    fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir_all(tmp.path().join("Downloads")).unwrap();
        Self { tmp }
    }

    fn private(&self) -> PathBuf {
        self.tmp.path().join("Private")
    }

    fn open(&self, fake: &Fake) -> Result<Arc<CoreHandle>, FfiError> {
        CoreHandle::open(
            s(&self.private()),
            s(&self.tmp.path().join("data")),
            Box::new(fake.clone()),
        )
    }

    fn download(&self, name: &str) -> String {
        let dst = self.tmp.path().join("Downloads").join(name);
        fs::copy(Path::new(SAMPLES).join(name), &dst).unwrap();
        s(&dst)
    }
}

fn s(p: &Path) -> String {
    p.to_str().unwrap().to_string()
}

fn settled(h: &CoreHandle) -> Vec<QueueItem> {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let q = h.queue();
        let docs: Vec<&DocItem> = q.iter().flat_map(|i| &i.children).collect();
        if !docs.is_empty() && docs.iter().all(|d| !d.state.in_progress()) {
            return q;
        }
        assert!(Instant::now() < deadline, "timed out; queue = {q:#?}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// OCR text is detected with less confidence, so the core may hold a value
/// for review. Either way it was read and classified, not failed.
fn assert_ocr_processed(q: &[QueueItem]) {
    let d = &q[0].children[0];
    assert!(
        matches!(d.state, DocState::Ready | DocState::NeedsAttention),
        "{d:#?}"
    );
    assert_eq!(d.kind_label, "W-2");
    assert!(!format!("{q:?}").contains(JORDAN_SSN));
}

#[test]
fn a_pdf_with_a_text_layer_goes_through_the_swift_text_source() {
    let w = World::new();
    let fake = Fake::new(Some(one_page(w2_text())), Err("unused".into()));
    let h = w.open(&fake).unwrap();
    h.ingest(vec![w.download("w2-acme-2025.pdf")], IngestMode::Move)
        .unwrap();

    let q = settled(&h);
    let d = &q[0].children[0];
    assert_eq!(d.state, DocState::Ready, "{d:#?}");
    assert_eq!(d.kind_label, "W-2");
    assert!(d.inked > 0);
    assert!(!format!("{q:?}").contains(JORDAN_SSN));

    let calls = fake.calls.lock().unwrap();
    assert_eq!(calls.key, 1, "the key is loaded once, at open");
    assert_eq!(calls.pdf_text.len(), 1);
    assert!(
        calls.pdf_text[0].starts_with(&s(&w.private())),
        "reads the moved file"
    );
    assert!(calls.ocr.is_empty());
}

#[test]
fn a_pdf_without_a_text_layer_falls_back_to_swift_ocr() {
    let w = World::new();
    let fake = Fake::new(None, Ok(one_page(w2_text())));
    let h = w.open(&fake).unwrap();
    h.ingest(vec![w.download("w2-acme-2025-scan.pdf")], IngestMode::Move)
        .unwrap();

    let q = settled(&h);
    assert_ocr_processed(&q);
    let calls = fake.calls.lock().unwrap();
    assert_eq!(calls.pdf_text.len(), 1);
    assert_eq!(calls.ocr.len(), 1);
}

#[test]
fn a_blank_text_layer_counts_as_none() {
    let w = World::new();
    let blank = ExtractedFfi {
        pages: vec![
            ExtractedPage {
                page: 1,
                text: "  \n".into(),
            },
            ExtractedPage {
                page: 2,
                text: String::new(),
            },
        ],
        fields: vec![],
    };
    let fake = Fake::new(Some(blank), Ok(one_page(w2_text())));
    let h = w.open(&fake).unwrap();
    h.ingest(vec![w.download("w2-acme-2025-scan.pdf")], IngestMode::Move)
        .unwrap();
    let q = settled(&h);
    assert_ocr_processed(&q);
    assert_eq!(fake.calls.lock().unwrap().ocr.len(), 1);
}

#[test]
fn form_fields_from_swift_are_inked() {
    let w = World::new();
    let with_fields = ExtractedFfi {
        pages: vec![ExtractedPage {
            page: 1,
            text: w2_text(),
        }],
        fields: vec![ExtractedField {
            name: "f1_01".into(),
            value: JORDAN_SSN.into(),
            page: Some(1),
        }],
    };
    let fake = Fake::new(Some(with_fields), Err("unused".into()));
    let h = w.open(&fake).unwrap();
    h.ingest(vec![w.download("w2-acme-2025.pdf")], IngestMode::Move)
        .unwrap();
    let q = settled(&h);
    assert_eq!(q[0].children[0].state, DocState::Ready, "{q:#?}");
    assert!(!format!("{q:?}").contains(JORDAN_SSN));
}

#[test]
fn a_swift_ocr_error_fails_the_document_without_leaking_its_message() {
    let w = World::new();
    let fake = Fake::new(None, Err("vision-internal-detail".into()));
    let h = w.open(&fake).unwrap();
    h.ingest(vec![w.download("w2-acme-2025-scan.pdf")], IngestMode::Move)
        .unwrap();
    let q = settled(&h);
    let d = &q[0].children[0];
    assert_eq!(d.state, DocState::Failed, "{d:#?}");
    assert!(!format!("{q:?}").contains("vision-internal-detail"));
}

#[test]
fn a_key_of_the_wrong_length_refuses_to_open() {
    let w = World::new();
    let mut fake = Fake::new(None, Err("unused".into()));
    fake.key = vec![1u8; 16];
    let err = w.open(&fake).unwrap_err();
    assert!(matches!(err, FfiError::Backend { .. }), "{err:?}");
}

#[test]
fn open_rejects_empty_folders() {
    let fake = Fake::new(None, Err("unused".into()));
    let err =
        CoreHandle::open(String::new(), "/nonexistent".into(), Box::new(fake.clone())).unwrap_err();
    assert!(matches!(err, FfiError::InvalidArgument { .. }));
    let err = CoreHandle::open("/nonexistent".into(), "  ".into(), Box::new(fake)).unwrap_err();
    assert!(matches!(err, FfiError::InvalidArgument { .. }));
}

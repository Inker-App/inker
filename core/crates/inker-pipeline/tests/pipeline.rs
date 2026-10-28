#![allow(clippy::expect_used, clippy::unwrap_used)]
//! End-to-end: a synthetic document goes from a "Downloads" folder through
//! ingest, extract, detect and tokenize, and comes out as a handle whose
//! resolved text carries tokens and no real values.

use std::fs;

use camino::{Utf8Path, Utf8PathBuf};
use inker_core::ports::HostPaths;
use inker_core::vault::MemoryKeyStore;
use inker_core::{Core, DocState, Error, Handle, IngestMode, Resolved};
use inker_detect::NoModel;
use inker_extract::{Extractor, FixtureOcr, PdfTextSource};
use inker_packs::PackSet;
use inker_pipeline::{Outcome, Pipeline};

const SAMPLES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../samples/synthetic");

/// Jordan Example's synthetic SSN, shared by the W-2 and the K-1 fixtures.
const JORDAN_SSN: &str = "900-12-3456";

struct World {
    _tmp: tempfile::TempDir,
    downloads: Utf8PathBuf,
    private: Utf8PathBuf,
    pipeline: Pipeline,
}

impl World {
    fn new() -> Self {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tmp.path().to_path_buf()).expect("utf8");
        let downloads = root.join("Downloads");
        let private = root.join("Private");
        let data = root.join("data");
        fs::create_dir_all(&downloads).expect("mkdir");
        fs::create_dir_all(&private).expect("mkdir");
        fs::create_dir_all(&data).expect("mkdir");
        let core = Core::open(
            HostPaths {
                private_folder: private.clone(),
                data_dir: data,
            },
            &MemoryKeyStore::new().expect("keystore"),
        )
        .expect("core");
        let extractor = Extractor::new(PdfTextSource::new(), FixtureOcr::new());
        let packs = PackSet::builtin().expect("packs");
        let pipeline = Pipeline::new(core, extractor, packs).with_model(Box::new(NoModel));
        Self {
            _tmp: tmp,
            downloads,
            private,
            pipeline,
        }
    }

    /// Copy a sample into the fake Downloads folder and return its path there.
    fn download(&self, name: &str) -> Utf8PathBuf {
        let src = Utf8Path::new(SAMPLES).join(name);
        let dst = self.downloads.join(name);
        fs::copy(&src, &dst).unwrap_or_else(|e| panic!("copy {src}: {e}"));
        dst
    }

    fn text_of(&self, handle: &Handle) -> String {
        match self.pipeline.core().resolve(handle).expect("resolve") {
            Resolved::Document(d) => d
                .pages
                .into_iter()
                .map(|p| p.text)
                .collect::<Vec<_>>()
                .join("\n"),
            Resolved::Folder(_) => panic!("expected a document"),
        }
    }
}

#[test]
fn w2_text_file_becomes_a_ready_handle_with_tokens_and_no_real_values() {
    let w = World::new();
    let src = w.download("w2-acme-2025.txt");

    let outcomes = w
        .pipeline
        .ingest(std::slice::from_ref(&src), IngestMode::Move)
        .expect("ingest");
    assert_eq!(outcomes.len(), 1);
    let Outcome { id, state, .. } = &outcomes[0];
    assert_eq!(
        *state,
        DocState::Ready,
        "W-2 fixture should not need review"
    );

    // Moved, not copied.
    assert!(!src.exists(), "original should be gone from Downloads");
    let rec = w.pipeline.core().document(*id).expect("record");
    assert!(
        rec.path.starts_with(&w.private),
        "lives in the private folder: {}",
        rec.path
    );
    assert!(
        rec.codename.as_str().starts_with("w2-"),
        "codename {}",
        rec.codename
    );
    assert_eq!(rec.year, Some(2025));
    assert!(
        rec.inked >= 3,
        "expected several inked values, got {}",
        rec.inked
    );

    let text = w.text_of(&Handle::new(rec.codename.clone()));
    assert!(text.contains("[SSN_"), "tokenized SSN missing:\n{text}");
    assert!(!text.contains(JORDAN_SSN), "real SSN leaked");
    assert!(
        !text.contains("900123456"),
        "real SSN leaked without separators"
    );
    assert!(
        text.contains("84,250.00"),
        "box 1 wages must be kept:\n{text}"
    );
    assert!(
        text.contains("11,912.40"),
        "box 2 withholding must be kept:\n{text}"
    );
}

#[test]
fn the_same_person_gets_the_same_token_across_documents() {
    let w = World::new();
    let w2 = w.download("w2-acme-2025.txt");
    let k1 = w.download("k1-tidewater-2025.txt");

    let outcomes = w
        .pipeline
        .ingest(&[w2, k1], IngestMode::Copy)
        .expect("ingest");
    assert_eq!(outcomes.len(), 2);
    let texts: Vec<String> = outcomes
        .iter()
        .map(|o| {
            let rec = w.pipeline.core().document(o.id).expect("record");
            w.text_of(&Handle::new(rec.codename))
        })
        .collect();
    let token_in = |t: &str| {
        let i = t.find("[SSN_").expect("an SSN token");
        let j = t[i..].find(']').expect("closing bracket") + i + 1;
        t[i..j].to_string()
    };
    assert_eq!(
        token_in(&texts[0]),
        token_in(&texts[1]),
        "Jordan must be the same token everywhere"
    );
}

#[test]
fn a_scan_goes_through_ocr_and_is_inked() {
    let w = World::new();
    // The fixture OCR looks for a sidecar beside the file it is given. The
    // pipeline extracts from the file's new home, so the sidecar must be
    // there. On macOS, Vision needs no sidecar.
    let src = w.download("w2-acme-2025-scan.pdf");
    let sidecar = Utf8Path::new(SAMPLES).join("w2-acme-2025-scan.ocr.json");
    fs::copy(&sidecar, w.private.join("w2-acme-2025-scan.ocr.json")).expect("sidecar");

    let outcomes = w.pipeline.ingest(&[src], IngestMode::Move).expect("ingest");
    let rec = w.pipeline.core().document(outcomes[0].id).expect("record");
    assert!(
        matches!(rec.state, DocState::Ready | DocState::NeedsAttention),
        "got {:?} ({:?})",
        rec.state,
        rec.status_note
    );
    if rec.state == DocState::Ready {
        let text = w.text_of(&Handle::new(rec.codename.clone()));
        assert!(!text.contains(JORDAN_SSN));
        assert!(text.contains("[SSN_"), "{text}");
    } else {
        let err = w
            .pipeline
            .core()
            .resolve(&Handle::new(rec.codename.clone()))
            .unwrap_err();
        assert!(matches!(err, Error::PendingReview(_)), "{err}");
    }
}

#[test]
fn an_unsupported_file_is_moved_but_marked_failed() {
    let w = World::new();
    let src = w.downloads.join("archive.zip");
    fs::write(&src, b"PK\x03\x04not really").expect("write");

    let outcomes = w
        .pipeline
        .ingest(std::slice::from_ref(&src), IngestMode::Move)
        .expect("ingest");
    assert_eq!(outcomes[0].state, DocState::Failed);
    assert!(!src.exists(), "still moved out of Downloads");
    let rec = w.pipeline.core().document(outcomes[0].id).expect("record");
    assert!(rec.path.starts_with(&w.private));
    let err = w
        .pipeline
        .core()
        .resolve(&Handle::new(rec.codename))
        .unwrap_err();
    assert!(matches!(err, Error::NotReady(_)), "{err}");
}

#[test]
fn an_unsure_value_holds_the_document_until_answered() {
    let w = World::new();
    let src = w.downloads.join("note.txt");
    // A long digit run with no label that any rule recognises and no checksum
    // to confirm it: the fail-closed pass must hold the document rather than
    // guess. (A labelled "account number" is inked confidently by the rules
    // and does not hold; that is tested in inker-detect.)
    fs::write(
        &src,
        "Wire confirmation 00000000000 received\nAmount 1,250.00\n",
    )
    .expect("write");

    let outcomes = w.pipeline.ingest(&[src], IngestMode::Move).expect("ingest");
    let rec = w.pipeline.core().document(outcomes[0].id).expect("record");
    assert_eq!(rec.state, DocState::NeedsAttention, "{:?}", rec.status_note);

    let handle = Handle::new(rec.codename.clone());
    assert!(matches!(
        w.pipeline.core().resolve(&handle).unwrap_err(),
        Error::PendingReview(_)
    ));

    let questions = w.pipeline.core().pending_reviews().expect("questions");
    assert_eq!(questions.len(), 1);
    let q = &questions[0];
    let prompt = q.prompt.clone().unwrap_or_default();
    assert!(
        !prompt.contains("00000000000"),
        "prompt must not contain the value: {prompt}"
    );

    let state = w
        .pipeline
        .core()
        .answer_review(q.id, inker_core::ReviewAnswer::InkIt)
        .expect("answer");
    assert_eq!(state, DocState::Ready);
    let text = w.text_of(&handle);
    assert!(
        !text.contains("00000000000"),
        "value must be inked after Ink it:\n{text}"
    );
    assert!(text.contains("1,250.00"), "amount kept");
}

#[test]
fn an_agent_output_is_never_reprocessed() {
    // Finding 8 of the security review: an output file is rehydrated on
    // disk. Re-extracting it would put whatever detection misses into the
    // tokenized view. `process` must refuse outputs and leave them as they are.
    let w = World::new();
    let src = w.download("w2-acme-2025.txt");
    let out = w
        .pipeline
        .ingest(std::slice::from_ref(&src), IngestMode::Copy)
        .expect("ingest");
    let rec = w.pipeline.core().document(out[0].id).expect("record");
    let handle = Handle::new(rec.codename);
    let written = w
        .pipeline
        .core()
        .write(&handle, "summary.md", "Employee [SSN_1] earned 84,250.00.")
        .expect("write");
    let output = w
        .pipeline
        .core()
        .find_document(&written.handle)
        .expect("output record");
    assert_eq!(output.state, DocState::Ready);

    let outcome = w.pipeline.process(output.id);
    let why = outcome.error.clone().unwrap_or_default();
    assert!(
        why.contains("output"),
        "process must refuse an output explicitly, got: {why:?}"
    );
    let after = w.pipeline.core().document(output.id).expect("record");
    assert_eq!(after.state, DocState::Ready, "state must be untouched");
    let text = w.text_of(&written.handle);
    assert!(
        text.contains("[SSN_1]") && !text.contains(JORDAN_SSN),
        "{text}"
    );
}

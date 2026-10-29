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

#[test]
fn a_document_left_mid_processing_is_recovered_and_processed() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = Utf8PathBuf::from_path_buf(tmp.path().to_path_buf()).expect("utf8");
    let downloads = root.join("Downloads");
    fs::create_dir_all(&downloads).expect("mkdir");
    let keys = MemoryKeyStore::with_key([7; 32]);
    let open = || {
        let core = Core::open(
            HostPaths {
                private_folder: root.join("Private"),
                data_dir: root.join("data"),
            },
            &keys,
        )
        .expect("core");
        let extractor = Extractor::new(PdfTextSource::new(), FixtureOcr::new());
        Pipeline::new(core, extractor, PackSet::builtin().expect("packs"))
            .with_model(Box::new(NoModel))
    };

    let src = downloads.join("w2-acme-2025.txt");
    fs::copy(Utf8Path::new(SAMPLES).join("w2-acme-2025.txt"), &src).expect("copy");
    let id = {
        let p = open();
        let id = p
            .core()
            .begin_ingest(&[src], IngestMode::Move)
            .expect("ingest")[0];
        // As if the app died while inking.
        p.core()
            .set_state(id, DocState::Reading, Some("reading"))
            .expect("reading");
        p.core()
            .set_state(id, DocState::Inking, Some("inking"))
            .expect("inking");
        id
    };
    // The first pipeline, and with it the lock on the data dir, is gone.
    let p = open();
    assert_eq!(
        p.core().document(id).expect("record").state,
        DocState::Inking
    );
    let recovered = p.recover().expect("recover");
    assert_eq!(recovered, vec![id]);
    assert_eq!(
        p.core().document(id).expect("record").state,
        DocState::Queued
    );
    assert!(p.recover().expect("recover twice").is_empty());

    let outcome = p.process(id);
    assert_eq!(outcome.state, DocState::Ready, "{:?}", outcome.error);
}

/// Every page and field of a Ready document's resolved view, as one string.
fn resolved_view(w: &World, handle: &Handle) -> String {
    match w.pipeline.core().resolve(handle).expect("resolve") {
        Resolved::Document(d) => {
            let mut out: Vec<String> = d.pages.into_iter().map(|p| p.text).collect();
            out.extend(d.fields.into_iter().map(|f| format!("{f:?}")));
            out.join("\n")
        }
        Resolved::Folder(_) => panic!("expected a document"),
    }
}

/// Ingest one sample and return its resolved view (pages plus fields).
fn ingest_and_view(w: &World, name: &str) -> String {
    let src = w.download(name);
    let outcomes = w
        .pipeline
        .ingest(std::slice::from_ref(&src), IngestMode::Copy)
        .expect("ingest");
    let id = outcomes[0].id;
    // A held document is answered "Ink it" throughout, as a user who inks
    // everything would.
    for q in w.pipeline.core().pending_reviews().expect("questions") {
        if q.document == id {
            w.pipeline
                .core()
                .answer_review(q.id, inker_core::ReviewAnswer::InkIt)
                .expect("answer");
        }
    }
    let rec = w.pipeline.core().document(id).expect("record");
    assert_eq!(
        rec.state,
        DocState::Ready,
        "{name} should be Ready: {:?}",
        rec.status_note
    );
    resolved_view(w, &Handle::new(rec.codename))
}

#[test]
fn the_w2_pdf_never_shows_any_part_of_the_employee_name() {
    // First real end-to-end run: box e's IRS layout prints the first name
    // and initial in one column and the last name in the next. The template
    // read only the first column, so "Example" passed through.
    let w = World::new();
    let view = ingest_and_view(&w, "w2-acme-2025.pdf");
    for part in ["Jordan", "Example"] {
        assert!(
            !view.contains(part),
            "name part {part:?} leaked from the W-2 PDF:\n{view}"
        );
    }
    assert!(view.contains("[PERSON_"), "the name is one token:\n{view}");
}

#[test]
fn no_sample_shows_any_part_of_the_synthetic_names() {
    let mut leaks = Vec::new();
    for name in [
        "w2-acme-2025.txt",
        "1098-harbor-2025.txt",
        "1099-int-harbor-2025.txt",
        "1099-int-harbor-2025.pdf",
        "k1-tidewater-2025.txt",
        "closing-statement-granite-2025.txt",
        "bank-statement-harbor-2025-03.txt",
        "1040-example-2023.txt",
    ] {
        let w = World::new();
        let view = ingest_and_view(&w, name);
        for part in ["Jordan", "Example"] {
            if let Some(line) = view.lines().find(|l| l.contains(part)) {
                leaks.push(format!("{name}: {part:?} in {line:?}"));
            }
        }
    }
    assert!(leaks.is_empty(), "name parts leaked:\n{}", leaks.join("\n"));
}

#[test]
fn a_joint_1040_is_recognised_and_every_identifier_is_inked() {
    // A dropped federal return was classified as a generic tax form and
    // its names came back as a stream of "can't place it" questions.
    let w = World::new();
    let src = w.download("1040-example-2023.txt");
    let outcomes = w
        .pipeline
        .ingest(std::slice::from_ref(&src), IngestMode::Copy)
        .expect("ingest");
    let id = outcomes[0].id;
    let questions: Vec<_> = w
        .pipeline
        .core()
        .pending_reviews()
        .expect("questions")
        .into_iter()
        .filter(|q| q.document == id)
        .map(|q| q.prompt.unwrap_or_default())
        .collect();
    assert!(questions.is_empty(), "questions on the 1040: {questions:?}");
    let rec = w.pipeline.core().document(id).expect("record");
    assert_eq!(rec.state, DocState::Ready, "{:?}", rec.status_note);
    assert_eq!(rec.kind.as_str(), "1040");
    assert_eq!(rec.year, Some(2023));
    assert!(
        rec.codename.as_str().starts_with("1040-2023"),
        "codename {}",
        rec.codename
    );

    let view = resolved_view(&w, &Handle::new(rec.codename));
    for real in [
        // Taxpayer, spouse and dependent: names and SSNs.
        "Jordan",
        "Example",
        JORDAN_SSN,
        "Riley",
        "Sample",
        "666-48-2210",
        "Casey",
        "900-12-7788",
        // Home address.
        "48 Lantern Way",
        // Direct deposit.
        "123456789",
        "00012345678",
        // Designee.
        "Morgan",
        "Placeholder",
        "(503) 555-0175",
        "40917",
        // Signature block.
        "Field Engineer",
        "Data Analyst",
        "000417",
        "(503) 555-0118",
        "jordan.example@example.com",
        // Paid preparer.
        "P00000000",
        "Ledgerline",
        "79-2020304",
        "12 Ledger Lane",
        "(541) 555-0190",
    ] {
        assert!(
            !view.contains(real),
            "{real:?} visible on the 1040:\n{view}"
        );
    }
    for token in ["[SSN_", "[PERSON_", "[ROUTING_", "[ACCOUNT_"] {
        assert!(view.contains(token), "no {token} token:\n{view}");
    }
    for kept in [
        "84,250.00",  // 1a wages
        "104,447.04", // 9 total income
        "103,140.04", // 11 adjusted gross income
        "75,440.04",  // 15 taxable income
        "8,628.00",   // 16 tax
        "9,242.00",   // 24 total tax
        "11,912.40",  // 25d withholding
        "2,670.40",   // 35a refund
    ] {
        assert!(
            view.contains(kept),
            "money line {kept} must be kept:\n{view}"
        );
    }
}

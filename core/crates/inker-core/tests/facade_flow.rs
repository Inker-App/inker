//! End-to-end run of the `Core` facade on a temp dir with a `MemoryKeyStore`.
//! Synthetic values only (SSN area 000/666, made-up names).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use camino::Utf8PathBuf;
use inker_core::vault::MemoryKeyStore;
use inker_core::{
    Core, Detection, DetectionSource, DocKind, DocState, Error, Extracted, Handle, IngestMode,
    PageText, Resolved, Span, TextOrigin, TokenKind, Treatment, ports::HostPaths,
};

const TEXT: &str =
    "Employee: Testy McFakename\nSSN: 000-12-3456\nEmployer: Acme Inc\nWages: 1234.00";

fn det(needle: &str, kind: TokenKind, role: &str) -> Detection {
    let start = TEXT.find(needle).unwrap();
    Detection {
        page: 1,
        span: Span {
            start,
            end: start + needle.len(),
        },
        kind,
        source: DetectionSource::Template {
            form: "w2".into(),
            field: role.into(),
        },
        confidence: 0.99,
        treatment: Treatment::Tokenize,
        needs_review: false,
        review_prompt: None,
        role: Some(role.into()),
        generalized: None,
        field: None,
    }
}

fn detections() -> Vec<Detection> {
    vec![
        det("Testy McFakename", TokenKind::Person, "employee"),
        det("000-12-3456", TokenKind::Ssn, "employee"),
        det("Acme Inc", TokenKind::Org, "employer"),
    ]
}

fn extracted() -> Extracted {
    Extracted {
        pages: vec![PageText {
            page: 1,
            text: TEXT.into(),
            origin: TextOrigin::TextLayer,
        }],
        fields: vec![],
    }
}

#[test]
fn whole_facade_flow_and_stable_tokens_across_reopen() {
    let tmp = tempfile::tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(tmp.path().canonicalize().unwrap()).unwrap();
    let paths = HostPaths {
        private_folder: root.join("Private"),
        data_dir: root.join("data"),
    };
    let downloads = root.join("Downloads");
    std::fs::create_dir_all(&downloads).unwrap();
    let src = downloads.join("w2 scan.txt");
    std::fs::write(&src, TEXT).unwrap();
    let ks = MemoryKeyStore::new().unwrap();

    let first_view;
    {
        let core = Core::open(paths.clone(), &ks).unwrap();
        let ids = core
            .begin_ingest(std::slice::from_ref(&src), IngestMode::Move)
            .unwrap();
        assert_eq!(ids.len(), 1);
        assert!(!src.exists(), "moved, not copied");
        let rec = core.document(ids[0]).unwrap();
        assert_eq!(rec.state, DocState::Queued);
        assert!(rec.path.starts_with(&paths.private_folder));

        // In flight: not served.
        let early = Handle::new(rec.codename.clone());
        assert!(matches!(core.resolve(&early), Err(Error::NotReady(_))));

        let state = core
            .complete_processing(
                ids[0],
                &extracted(),
                &detections(),
                DocKind("w2".into()),
                Some(2025),
            )
            .unwrap();
        assert_eq!(state, DocState::Ready);
        let rec = core.document(ids[0]).unwrap();
        assert_eq!(rec.codename.as_str(), "w2-2025");
        assert_eq!(rec.inked, 3);

        let handle = Handle::parse("🐙/w2-2025").unwrap();
        let Resolved::Document(doc) = core.resolve(&handle).unwrap() else {
            panic!("expected a document")
        };
        let text = &doc.pages[0].text;
        assert_eq!(
            text,
            "Employee: [PERSON_1]\nSSN: [SSN_1]\nEmployer: [ORG_1]\nWages: 1234.00"
        );
        assert!(!text.contains("McFakename") && !text.contains("000-12-3456"));
        first_view = text.clone();

        let desc = core.describe(&handle).unwrap();
        assert_eq!(desc.state, DocState::Ready);
        assert!(
            desc.roles
                .iter()
                .any(|r| r.token.to_string() == "[SSN_1]" && r.role == "employee")
        );
        assert!(
            desc.roles
                .iter()
                .any(|r| r.token.to_string() == "[ORG_1]" && r.role == "employer")
        );

        let list = core.list().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].handle, handle);

        // write: rehydrated on disk, tokenized for the agent.
        let out = core
            .write(
                &handle,
                "summary.md",
                "[PERSON_1] ([SSN_1]) works at [ORG_1].",
            )
            .unwrap();
        assert_eq!(out.handle.to_string(), "🐙/w2-2025-summary");
        assert!(out.path.starts_with(&paths.private_folder));
        assert_eq!(
            std::fs::read_to_string(&out.path).unwrap(),
            "Testy McFakename (000-12-3456) works at Acme Inc."
        );
        let Resolved::Document(o) = core.resolve(&out.handle).unwrap() else {
            panic!()
        };
        assert_eq!(o.pages[0].text, "[PERSON_1] ([SSN_1]) works at [ORG_1].");

        // Escapes are refused.
        assert!(matches!(
            core.write(&handle, "../x.md", "x"),
            Err(Error::Refused(_))
        ));

        core.record_agent_read("Claude Code", &handle).unwrap();
        assert_eq!(core.recent_reads(5).unwrap()[0].agent, "Claude Code");
    }

    // Reopen the same data dir: same tokens, numbering continues.
    let core = Core::open(paths.clone(), &ks).unwrap();
    let handle = Handle::parse("ink/w2-2025").unwrap();
    let Resolved::Document(doc) = core.resolve(&handle).unwrap() else {
        panic!()
    };
    assert_eq!(doc.pages[0].text, first_view);

    let src2 = downloads.join("1099.txt");
    let text2 = "Payee SSN 000123456 and 666-00-0001";
    std::fs::write(&src2, text2).unwrap();
    let ids = core.begin_ingest(&[src2], IngestMode::Copy).unwrap();
    let mk = |needle: &str| {
        let start = text2.find(needle).unwrap();
        Detection {
            span: Span {
                start,
                end: start + needle.len(),
            },
            role: None,
            ..det("000-12-3456", TokenKind::Ssn, "x")
        }
    };
    let ex = Extracted {
        pages: vec![PageText {
            page: 1,
            text: text2.into(),
            origin: TextOrigin::TextLayer,
        }],
        fields: vec![],
    };
    core.complete_processing(
        ids[0],
        &ex,
        &[mk("000123456"), mk("666-00-0001")],
        DocKind("1099-int".into()),
        None,
    )
    .unwrap();
    let rec = core.document(ids[0]).unwrap();
    let Resolved::Document(d2) = core.resolve(&Handle::new(rec.codename)).unwrap() else {
        panic!()
    };
    assert_eq!(d2.pages[0].text, "Payee SSN [SSN_1] and [SSN_2]");
    assert_eq!(core.recent_reads(5).unwrap().len(), 1);
}

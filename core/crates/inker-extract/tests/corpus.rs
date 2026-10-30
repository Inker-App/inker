//! Extraction against the synthetic corpus in `samples/synthetic/`.
//! Regenerate it with `python3 -I samples/generate.py`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;

use camino::{Utf8Path, Utf8PathBuf};
use inker_core::{
    Error, Extracted, FormField, PageText, Result, TextOrigin,
    ports::{OcrSource, TextSource},
};
use inker_extract::{Extractor, FileKind, FixtureOcr, NoOcr, PdfTextSource, detect_file_kind};

// Synthetic values from samples/generate.py.
const JORDAN_SSN: &str = "900-12-3456";
const RILEY_SSN: &str = "666-48-2210";
const BLUEFIN_EIN: &str = "89-7654321";

fn sample(name: &str) -> Utf8PathBuf {
    let p = Utf8Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../samples/synthetic")
        .join(name);
    assert!(
        p.exists(),
        "{p} missing: run python3 -I samples/generate.py"
    );
    p
}

/// A scratch dir under the target dir, unique per test.
fn scratch(test: &str) -> Utf8PathBuf {
    let d = Utf8Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("inker-extract")
        .join(test);
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

fn is_extract_err<T: std::fmt::Debug>(r: Result<T>) -> bool {
    matches!(r, Err(Error::Extract(_)))
}

fn field<'a>(fields: &'a [FormField], name: &str) -> &'a FormField {
    fields.iter().find(|f| f.name == name).unwrap_or_else(|| {
        panic!(
            "no field {name} in {:?}",
            fields.iter().map(|f| &f.name).collect::<Vec<_>>()
        )
    })
}

// ---------------------------------------------------------------------------
// detect_file_kind
// ---------------------------------------------------------------------------

#[test]
fn kind_of_corpus_files() {
    assert_eq!(detect_file_kind(&sample("w2-acme-2025.pdf")), FileKind::Pdf);
    assert_eq!(
        detect_file_kind(&sample("w2-acme-2025-scan.pdf")),
        FileKind::Pdf
    );
    assert_eq!(
        detect_file_kind(&sample("receipt-cornerstone-2025-03-09-photo.png")),
        FileKind::Image
    );
    assert_eq!(
        detect_file_kind(&sample("w2-acme-2025.txt")),
        FileKind::Text
    );
    assert_eq!(
        detect_file_kind(&sample("w2-acme-2025.ocr.json")),
        FileKind::Unsupported
    );
}

#[test]
fn kind_by_magic_bytes() {
    let d = scratch("kind_by_magic_bytes");
    let cases: &[(&str, &[u8], FileKind)] = &[
        (
            "photo.jpg",
            b"\xFF\xD8\xFF\xE0\x00\x10JFIF\x00",
            FileKind::Image,
        ),
        (
            "photo.jpeg",
            b"\xFF\xD8\xFF\xE1\x00\x10Exif\x00",
            FileKind::Image,
        ),
        (
            "IMG_0001.HEIC",
            b"\x00\x00\x00\x18ftypheic\x00\x00\x00\x00",
            FileKind::Image,
        ),
        (
            "IMG_0002.heic",
            b"\x00\x00\x00\x18ftypmif1\x00\x00\x00\x00",
            FileKind::Image,
        ),
        // Magic wins over a wrong or missing extension.
        ("download", b"%PDF-1.7\n", FileKind::Pdf),
        ("scan.dat", b"\x89PNG\r\n\x1a\n", FileKind::Image),
        // An extension that lies about a document is not trusted.
        ("fake.pdf", b"hello, not a pdf", FileKind::Unsupported),
        ("fake.png", b"hello, not a png", FileKind::Unsupported),
        // Text by extension, case-insensitive.
        ("notes.md", b"# notes\n", FileKind::Text),
        ("ledger.CSV", b"a,b\n1,2\n", FileKind::Text),
        ("empty.txt", b"", FileKind::Text),
        // Binary content behind a text extension is not text.
        ("blob.txt", b"\x00\x01\x02\x03", FileKind::Unsupported),
        ("archive.zip", b"PK\x03\x04", FileKind::Unsupported),
    ];
    for (name, bytes, want) in cases {
        let p = d.join(name);
        fs::write(&p, bytes).unwrap();
        assert_eq!(detect_file_kind(&p), *want, "{name}");
    }
}

#[test]
fn kind_of_missing_file_falls_back_to_extension() {
    let d = scratch("kind_of_missing_file");
    assert_eq!(detect_file_kind(&d.join("gone.txt")), FileKind::Text);
    assert_eq!(detect_file_kind(&d.join("gone.pdf")), FileKind::Pdf);
    assert_eq!(detect_file_kind(&d.join("gone.png")), FileKind::Image);
    assert_eq!(detect_file_kind(&d.join("gone.exe")), FileKind::Unsupported);
}

// ---------------------------------------------------------------------------
// PdfTextSource
// ---------------------------------------------------------------------------

#[test]
fn pdf_w2_text_layer() {
    let got = PdfTextSource::new()
        .extract(&sample("w2-acme-2025.pdf"))
        .unwrap()
        .expect("has a text layer");
    assert_eq!(got.pages.len(), 1);
    let page = &got.pages[0];
    assert_eq!(page.page, 1);
    assert_eq!(page.origin, TextOrigin::TextLayer);
    for needle in [
        "a Employee's social security number",
        JORDAN_SSN,
        "b Employer identification number (EIN)",
        "07-1234567",
        "Acme Robotics Inc",
        "1 Wages, tips, other compensation",
        "84,250.00",
    ] {
        assert!(
            page.text.contains(needle),
            "missing {needle:?} in:\n{}",
            page.text
        );
    }
    assert!(got.fields.is_empty());
}

#[test]
fn pdf_1099_int_text_layer() {
    let got = PdfTextSource::new()
        .extract(&sample("1099-int-harbor-2025.pdf"))
        .unwrap()
        .unwrap();
    let text = &got.pages[0].text;
    assert!(text.contains("1 Interest income"), "{text}");
    assert!(text.contains("Harbor Street Credit Union"));
    assert!(text.contains(JORDAN_SSN));
}

#[test]
fn pdf_text_matches_txt_rendition_line_by_line() {
    let got = PdfTextSource::new()
        .extract(&sample("w2-acme-2025.pdf"))
        .unwrap()
        .unwrap();
    let txt = fs::read_to_string(sample("w2-acme-2025.txt")).unwrap();
    let norm = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");
    assert_eq!(norm(&got.pages[0].text), norm(&txt));
}

#[test]
fn pdf_scan_without_text_layer_is_none() {
    assert_eq!(
        PdfTextSource::new()
            .extract(&sample("w2-acme-2025-scan.pdf"))
            .unwrap(),
        None
    );
}

#[test]
fn pdf_fillable_w2_reads_acroform() {
    let got = PdfTextSource::new()
        .extract(&sample("w2-bluefin-2025-fillable.pdf"))
        .unwrap()
        .unwrap();
    // Labels in the text layer, values only in the fields.
    assert!(
        got.pages[0]
            .text
            .contains("a Employee's social security number")
    );
    assert!(!got.pages[0].text.contains(RILEY_SSN));

    let f = &got.fields;
    assert_eq!(f.len(), 8, "{f:?}");
    assert_eq!(field(f, "w2.box_a_ssn").value, RILEY_SSN);
    assert_eq!(field(f, "w2.box_a_ssn").page, Some(1));
    assert_eq!(field(f, "w2.box_b_ein").value, BLUEFIN_EIN);
    assert_eq!(field(f, "w2.box_1_wages").value, "96,400.00");
    // UTF-16BE value.
    assert_eq!(field(f, "w2.box_e_name").value, "Riley Sample Ñandú");
    // Choice field.
    assert_eq!(field(f, "w2.box_15_state").value, "CO");
}

#[test]
fn pdf_source_rejects_non_pdf() {
    assert!(is_extract_err(
        PdfTextSource::new().extract(&sample("w2-acme-2025.txt"))
    ));
}

#[test]
fn pdf_source_missing_file_is_an_error() {
    let d = scratch("pdf_missing");
    assert!(PdfTextSource::new().extract(&d.join("nope.pdf")).is_err());
}

#[test]
fn pdf_source_truncated_pdf_is_an_error_not_a_panic() {
    let d = scratch("pdf_truncated");
    let bytes = fs::read(sample("w2-acme-2025.pdf")).unwrap();
    let p = d.join("cut.pdf");
    fs::write(&p, &bytes[..bytes.len() / 3]).unwrap();
    assert!(PdfTextSource::new().extract(&p).is_err());
}

// ---------------------------------------------------------------------------
// FixtureOcr and NoOcr
// ---------------------------------------------------------------------------

#[test]
fn fixture_ocr_reads_sidecar_for_scan_pdf() {
    let got = FixtureOcr::new()
        .ocr(&sample("w2-acme-2025-scan.pdf"))
        .unwrap();
    assert_eq!(got.pages.len(), 1);
    assert_eq!(got.pages[0].page, 1);
    assert_eq!(got.pages[0].origin, TextOrigin::Ocr);
    assert!(got.pages[0].text.contains("84,25O.OO"));
    assert!(got.pages[0].text.contains("900123456"));
    assert!(got.fields.is_empty());
}

#[test]
fn fixture_ocr_reads_sidecar_for_image() {
    let got = FixtureOcr::new()
        .ocr(&sample("receipt-cornerstone-2025-03-09-photo.png"))
        .unwrap();
    assert!(got.pages[0].text.contains("CORNERSTONE HARDWARE CO"));
    assert_eq!(got.pages[0].origin, TextOrigin::Ocr);
}

#[test]
fn fixture_ocr_every_corpus_sidecar_parses() {
    let dir = sample("");
    let mut n = 0;
    for entry in fs::read_dir(&dir).unwrap() {
        let p = Utf8PathBuf::from_path_buf(entry.unwrap().path()).unwrap();
        let name = p.file_name().unwrap().to_string();
        if let Some(stem) = name.strip_suffix(".ocr.json") {
            // Ask for OCR of the matching .txt rendition; the sidecar is beside it.
            let target = dir.join(format!("{stem}.png"));
            let got = FixtureOcr::new().ocr(&target).unwrap();
            assert!(!got.pages.is_empty(), "{name}");
            assert!(got.pages.iter().all(|p| p.origin == TextOrigin::Ocr));
            n += 1;
        }
    }
    assert_eq!(n, 14);
}

#[test]
fn w9_pdf_and_sidecar_have_every_page() {
    // Form W-9: the form, then two pages of instructions.
    let pdf = PdfTextSource::new()
        .extract(&sample("w9-bluefin-2025.pdf"))
        .unwrap()
        .unwrap();
    assert_eq!(
        pdf.pages.iter().map(|p| p.page).collect::<Vec<_>>(),
        [1, 2, 3]
    );
    assert!(pdf.pages[0].text.contains("Request for Taxpayer"));
    assert!(pdf.pages[2].text.contains("Secure Your Tax Records"));
    let ocr = FixtureOcr::new()
        .ocr(&sample("w9-bluefin-2025.pdf"))
        .unwrap();
    assert_eq!(ocr.pages.len(), 3);
}

#[test]
fn fixture_ocr_sorts_pages_and_keeps_numbers() {
    let d = scratch("fixture_ocr_pages");
    fs::write(d.join("two.pdf"), b"%PDF-1.7\n").unwrap();
    fs::write(
        d.join("two.ocr.json"),
        r#"{ "pages": [ { "page": 2, "text": "second" }, { "page": 1, "text": "first" } ] }"#,
    )
    .unwrap();
    let got = FixtureOcr::new().ocr(&d.join("two.pdf")).unwrap();
    let pages: Vec<(u32, &str)> = got
        .pages
        .iter()
        .map(|p| (p.page, p.text.as_str()))
        .collect();
    assert_eq!(pages, vec![(1, "first"), (2, "second")]);
}

#[test]
fn fixture_ocr_missing_sidecar_is_extract_error() {
    let d = scratch("fixture_ocr_missing");
    fs::write(d.join("lonely.png"), b"\x89PNG\r\n\x1a\n").unwrap();
    assert!(is_extract_err(FixtureOcr::new().ocr(&d.join("lonely.png"))));
}

#[test]
fn fixture_ocr_malformed_sidecar_is_extract_error() {
    let d = scratch("fixture_ocr_malformed");
    fs::write(d.join("bad.png"), b"\x89PNG\r\n\x1a\n").unwrap();
    fs::write(d.join("bad.ocr.json"), r#"{ "pages": "nope" }"#).unwrap();
    assert!(is_extract_err(FixtureOcr::new().ocr(&d.join("bad.png"))));
    fs::write(
        d.join("bad.ocr.json"),
        r#"{ "pages": [ { "page": 0, "text": "x" } ] }"#,
    )
    .unwrap();
    assert!(
        is_extract_err(FixtureOcr::new().ocr(&d.join("bad.png"))),
        "pages are 1-based"
    );
}

#[test]
fn no_ocr_always_fails() {
    assert!(is_extract_err(
        NoOcr.ocr(&sample("receipt-cornerstone-2025-03-09-photo.png"))
    ));
}

// ---------------------------------------------------------------------------
// Extractor
// ---------------------------------------------------------------------------

fn pure_rust() -> Extractor {
    Extractor::new(PdfTextSource::new(), FixtureOcr::new())
}

#[test]
fn extractor_pdf_with_text_layer_uses_text() {
    let got = pure_rust().extract(&sample("w2-acme-2025.pdf")).unwrap();
    assert_eq!(got.pages[0].origin, TextOrigin::TextLayer);
    assert!(got.pages[0].text.contains(JORDAN_SSN));
}

#[test]
fn extractor_scan_pdf_falls_back_to_ocr() {
    let got = pure_rust()
        .extract(&sample("w2-acme-2025-scan.pdf"))
        .unwrap();
    assert_eq!(got.pages[0].origin, TextOrigin::Ocr);
    assert!(got.pages[0].text.contains("Acme Robotics Inc"));
}

#[test]
fn extractor_image_uses_ocr() {
    let got = pure_rust()
        .extract(&sample("receipt-cornerstone-2025-03-09-photo.png"))
        .unwrap();
    assert_eq!(got.pages[0].origin, TextOrigin::Ocr);
}

#[test]
fn extractor_text_file_is_one_text_layer_page() {
    for stem in [
        "w2-acme-2025",
        "1099-int-harbor-2025",
        "1099-nec-bluefin-2025",
        "1099-div-northwind-2025",
        "1098-harbor-2025",
        "k1-tidewater-2025",
        "bank-statement-harbor-2025-03",
        "closing-statement-granite-2025",
        "receipt-cornerstone-2025-03-09",
    ] {
        let p = sample(&format!("{stem}.txt"));
        let got = pure_rust().extract(&p).unwrap();
        assert_eq!(
            got,
            Extracted {
                pages: vec![PageText {
                    page: 1,
                    text: fs::read_to_string(&p).unwrap(),
                    origin: TextOrigin::TextLayer
                }],
                fields: vec![],
            },
            "{stem}"
        );
    }
}

#[test]
fn extractor_same_ssn_in_w2_and_k1() {
    let w2 = pure_rust().extract(&sample("w2-acme-2025.txt")).unwrap();
    let k1 = pure_rust()
        .extract(&sample("k1-tidewater-2025.txt"))
        .unwrap();
    assert!(w2.full_text().contains(JORDAN_SSN));
    assert!(k1.full_text().contains(JORDAN_SSN));
}

#[test]
fn extractor_unsupported_is_extract_error() {
    assert!(is_extract_err(
        pure_rust().extract(&sample("w2-acme-2025.ocr.json"))
    ));
}

#[test]
fn extractor_non_utf8_text_is_extract_error() {
    let d = scratch("extractor_non_utf8");
    fs::write(d.join("latin1.txt"), b"caf\xe9\n").unwrap();
    assert!(is_extract_err(pure_rust().extract(&d.join("latin1.txt"))));
}

#[test]
fn extractor_scan_without_ocr_is_extract_error() {
    let e = Extractor::new(PdfTextSource::new(), NoOcr);
    assert!(is_extract_err(e.extract(&sample("w2-acme-2025-scan.pdf"))));
    // A PDF with a text layer still works without OCR.
    assert!(e.extract(&sample("w2-acme-2025.pdf")).is_ok());
}

/// The extractor trusts the TextSource's `None`, whichever source it is.
#[test]
fn extractor_composes_any_sources() {
    struct NeverText;
    impl TextSource for NeverText {
        fn extract(&self, _: &Utf8Path) -> Result<Option<Extracted>> {
            Ok(None)
        }
    }
    struct CannedOcr;
    impl OcrSource for CannedOcr {
        fn ocr(&self, _: &Utf8Path) -> Result<Extracted> {
            Ok(Extracted {
                pages: vec![PageText {
                    page: 1,
                    text: "canned".into(),
                    origin: TextOrigin::Ocr,
                }],
                fields: vec![],
            })
        }
    }
    let got = Extractor::new(NeverText, CannedOcr)
        .extract(&sample("w2-acme-2025.pdf"))
        .unwrap();
    assert_eq!(got.pages[0].text, "canned");
}

mod quiet_extraction {
    //! Finding 9 of the security review: pdf-extract and lopdf can print
    //! document bytes through panics and log lines. Extraction must stay
    //! silent and its errors must never quote the file.
    use camino::Utf8PathBuf;
    use inker_core::ports::TextSource;
    use inker_extract::{PdfTextSource, should_silence_panic};

    #[test]
    fn panics_from_the_pdf_libraries_are_silenced_and_ours_are_not() {
        assert!(should_silence_panic(
            "/home/x/.cargo/registry/src/index/pdf-extract-0.12.1/src/lib.rs"
        ));
        assert!(should_silence_panic(
            "/home/x/.cargo/registry/src/index/lopdf-0.42.0/src/parser.rs"
        ));
        assert!(!should_silence_panic("crates/inker-extract/src/pdf.rs"));
        assert!(!should_silence_panic(
            "/home/x/.cargo/registry/src/index/serde_json-1.0.0/src/de.rs"
        ));
    }

    #[test]
    fn a_broken_pdf_errors_without_quoting_its_content() {
        let dir = tempfile::tempdir().unwrap();
        let path = Utf8PathBuf::from_path_buf(dir.path().join("broken.pdf")).unwrap();
        let marker = "ZQX-SENTINEL-900-12-3456";
        let junk = format!(
            "%PDF-1.4\n1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n3 0 obj\n<< /Type /Page /Parent 2 0 R /Contents 4 0 R >>\nendobj\n4 0 obj\n<< /Length 999 >>\nstream\nBT ({marker}) Tj [ ( {marker} ) ] TJ ET garbage (({marker}\nendstream\nendobj\ntrailer\n<< /Root 1 0 R >>\n%%EOF\n"
        );
        std::fs::write(&path, junk).unwrap();
        let r = PdfTextSource::new().extract(&path);
        let msg = match r {
            Ok(_) => return, // tolerant parse is fine too; nothing to leak through an error
            Err(e) => e.to_string(),
        };
        assert!(
            !msg.contains(marker),
            "error quoted document content: {msg}"
        );
        assert!(
            !msg.contains("900-12-3456"),
            "error quoted document content: {msg}"
        );
    }
}

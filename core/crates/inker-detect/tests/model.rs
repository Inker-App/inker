#![allow(clippy::unwrap_used, clippy::expect_used)] // tests may panic

use inker_core::TokenKind;
use inker_detect::model::{GLINER_LABELS, map_label};

#[test]
fn every_requested_label_maps() {
    let want = [
        TokenKind::Ssn,
        TokenKind::Routing,
        TokenKind::Account,
        TokenKind::Person,
        TokenKind::Org,
        TokenKind::Address,
        TokenKind::Email,
        TokenKind::Phone,
    ];
    let got: Vec<TokenKind> = GLINER_LABELS
        .iter()
        .map(|l| map_label(l).unwrap())
        .collect();
    assert_eq!(got, want);
}

#[test]
fn label_variants_and_unknowns() {
    assert_eq!(map_label("Phone Number"), Some(TokenKind::Phone));
    assert_eq!(map_label("account number"), Some(TokenKind::Account));
    assert_eq!(map_label("social security number"), Some(TokenKind::Ssn));
    assert_eq!(map_label(" Organization "), Some(TokenKind::Org));
    assert_eq!(map_label("vehicle"), None);
    assert_eq!(map_label(""), None);
}

#[test]
fn chunks_split_at_lines_and_keep_offsets() {
    use inker_detect::model::chunks;
    let text = "aaaa\nbbbb\ncccc\ndd";
    let c = chunks(text, 10);
    assert_eq!(c, vec![(0, "aaaa\nbbbb\n"), (10, "cccc\ndd")]);
    for (off, s) in &c {
        assert_eq!(&text[*off..*off + s.len()], *s);
    }
    // A single line longer than the limit is cut at a char boundary.
    let long = "é".repeat(8);
    let c = chunks(&long, 5);
    assert!(c.iter().all(|(_, s)| s.len() <= 5 && !s.is_empty()));
    assert_eq!(c.iter().map(|(_, s)| s.len()).sum::<usize>(), long.len());
    assert!(chunks("", 10).is_empty());
}

#[cfg(feature = "gliner")]
#[test]
fn gliner_missing_model_is_an_error() {
    use inker_detect::model::{GlinerConfig, GlinerModel};
    let cfg = GlinerConfig {
        tokenizer: "/nonexistent/tokenizer.json".into(),
        model: "/nonexistent/model.onnx".into(),
        threshold: 0.5,
    };
    let e = GlinerModel::load(&cfg).unwrap_err();
    assert!(e.to_string().starts_with("detect: gliner:"), "{e}");
}

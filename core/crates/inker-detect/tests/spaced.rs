//! Letter-spaced text: a PDF text layer whose glyphs are placed one by one
//! (or typed into comb boxes) reads back as "9 0 0 - 1 2 - 3 4 5 6". The
//! detectors see a view with those spaces closed.
//!
//! Values are synthetic: SSN area 900, invented names, made-up PINs.

#![allow(clippy::unwrap_used, clippy::expect_used)] // tests may panic

use inker_detect::spaced::despace;

fn closed(text: &str) -> String {
    despace(text).map_or_else(|| text.to_string(), |d| d.text)
}

#[test]
fn ordinary_text_is_left_alone() {
    for text in [
        "Taxpayer's name   Jordan A Example   Social security number   900-12-3456",
        "16 Tax (see instructions). Check if any from Form(s): 1 [ ] 8814  2 [ ] 4972  3 [ ]",
        "[X] Married filing jointly   [ ] Head of household (HOH)",
        "Card ending **** **** **** 4242",
        "Lines 1 2 3 and 4 of the worksheet",
        "",
    ] {
        assert!(
            despace(text).is_none(),
            "{text:?} changed to {:?}",
            closed(text)
        );
    }
}

#[test]
fn letter_spaced_digits_close_up_on_any_line() {
    assert_eq!(
        closed("Social security number   9 0 0 - 1 2 - 3 4 5 6"),
        "Social security number   900-12-3456"
    );
    assert_eq!(
        closed("to enter or generate my PIN   1 3 5 7 9   as my signature"),
        "to enter or generate my PIN   13579   as my signature"
    );
}

#[test]
fn a_letter_spaced_line_closes_up_its_words() {
    assert_eq!(
        closed("J o r d a n A Ex a mp l e 9 0 0 - 1 2 - 3 4 5 6"),
        "Jordan AExample 900-12-3456"
    );
    assert_eq!(
        closed("Fo r m 8 8 7 9 I RS e - f i l e Si g n a t u r e Au t h o r i z a t i o n"),
        "Form 8879 IRS e-file Signature Authorization"
    );
    assert_eq!(
        closed("t o e n t e r o r g e n e r a t e my PI N 1 3 5 7 9"),
        "toenterorgeneratemy PIN 13579"
    );
    // An acronym ends where lower case starts again.
    assert_eq!(
        closed("L e d g e r l i n e Ta x Se r v i c e s L L C t o e n t e r"),
        "Ledgerline Tax Services LLC toenter"
    );
    // Two kerned pairs side by side ("Sa mp") are still one word.
    assert_eq!(
        closed("Ri l e y Sa mp l e 6 6 6 - 4 8 - 2 2 1 0"),
        "Riley Sample 666-48-2210"
    );
    assert_eq!(
        closed(
            "ERO' s EFI N/ PI N. En t e r y o u r s i x - d i g i t EFI N 0 0 0 0 0 0 9 7 5 3 1"
        ),
        "ERO's EFIN/PIN.Enteryoursix-digit EFIN 00000097531"
    );
}

#[test]
fn wider_gaps_on_a_letter_spaced_line_stay() {
    assert_eq!(
        closed("Ta x p a y e r ' s   n a me       J o r d a n   A  Ex a mp l e"),
        "Taxpayer's   name       Jordan   A  Example"
    );
}

#[test]
fn spans_map_back_to_the_original_text() {
    let text = "Name: J o r d a n A Ex a mp l e\nSSN 9 0 0 - 1 2 - 3 4 5 6\n";
    let d = despace(text).unwrap();
    let at = d.text.find("900-12-3456").unwrap();
    let (s, e) = d.span_to_orig(at, at + "900-12-3456".len());
    assert_eq!(&text[s..e], "9 0 0 - 1 2 - 3 4 5 6");
    let at = d.text.find('\n').unwrap();
    assert_eq!(
        d.span_to_orig(at, at + 1),
        (text.find('\n').unwrap(), text.find('\n').unwrap() + 1)
    );
    assert_eq!(d.span_to_orig(0, 4), (0, 4));
}

#[test]
fn non_ascii_text_maps_back_on_char_boundaries() {
    let text = "Ñ a n d ú R i l e y 6 6 6 4 8 2 2 1 0";
    let d = despace(text).unwrap();
    let at = d.text.find("666482210").unwrap();
    let (s, e) = d.span_to_orig(at, at + 9);
    assert_eq!(&text[s..e], "6 6 6 4 8 2 2 1 0");
    let (s, e) = d.span_to_orig(0, d.text.find(' ').unwrap());
    assert!(text.is_char_boundary(s) && text.is_char_boundary(e));
}

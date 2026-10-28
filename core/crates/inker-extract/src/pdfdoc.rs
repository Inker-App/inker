//! PDF string decoding: PDFDocEncoding, UTF-16BE with BOM, and (PDF 2.0)
//! UTF-8 with BOM. ISO 32000-2, Annex D.

const REPLACEMENT: char = '\u{FFFD}';

/// PDFDocEncoding for 0x18..=0x1F.
const LOW: [char; 8] = [
    '\u{02D8}', '\u{02C7}', '\u{02C6}', '\u{02D9}', '\u{02DD}', '\u{02DB}', '\u{02DA}', '\u{02DC}',
];

/// PDFDocEncoding for 0x80..=0xA0. 0x9F is undefined.
const HIGH: [char; 33] = [
    '\u{2022}',
    '\u{2020}',
    '\u{2021}',
    '\u{2026}',
    '\u{2014}',
    '\u{2013}',
    '\u{0192}',
    '\u{2044}', //
    '\u{2039}',
    '\u{203A}',
    '\u{2212}',
    '\u{2030}',
    '\u{201E}',
    '\u{201C}',
    '\u{201D}',
    '\u{2018}', //
    '\u{2019}',
    '\u{201A}',
    '\u{2122}',
    '\u{FB01}',
    '\u{FB02}',
    '\u{0141}',
    '\u{0152}',
    '\u{0160}', //
    '\u{0178}',
    '\u{017D}',
    '\u{0131}',
    '\u{0142}',
    '\u{0153}',
    '\u{0161}',
    '\u{017E}',
    REPLACEMENT, //
    '\u{20AC}',
];

fn pdfdoc_char(b: u8) -> char {
    match b {
        0x18..=0x1F => LOW[usize::from(b - 0x18)],
        0x7F => REPLACEMENT,
        0x80..=0xA0 => HIGH[usize::from(b - 0x80)],
        _ => char::from(b),
    }
}

/// Decode the bytes of a PDF text string to Unicode. Never panics; invalid
/// sequences become U+FFFD.
pub(crate) fn decode_pdf_string(bytes: &[u8]) -> String {
    if let Some(rest) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        let (pairs, tail) = rest.as_chunks::<2>();
        let units = pairs.iter().map(|&pair| u16::from_be_bytes(pair));
        let mut s: String = char::decode_utf16(units)
            .map(|r| r.unwrap_or(REPLACEMENT))
            .collect();
        if !tail.is_empty() {
            s.push(REPLACEMENT);
        }
        return s;
    }
    if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8_lossy(rest).into_owned();
    }
    bytes.iter().copied().map(pdfdoc_char).collect()
}
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::decode_pdf_string;

    #[test]
    fn ascii_is_unchanged() {
        assert_eq!(decode_pdf_string(b"96,400.00"), "96,400.00");
        assert_eq!(decode_pdf_string(b""), "");
    }

    #[test]
    fn utf16be_with_bom() {
        assert_eq!(
            decode_pdf_string(&[0xFE, 0xFF, 0x00, 0xD1, 0x00, 0x61]),
            "Ña"
        );
        // A surrogate pair: U+1F419 OCTOPUS.
        assert_eq!(
            decode_pdf_string(&[0xFE, 0xFF, 0xD8, 0x3D, 0xDC, 0x19]),
            "🐙"
        );
    }

    #[test]
    fn utf16be_odd_length_or_lone_surrogate_does_not_panic() {
        assert_eq!(
            decode_pdf_string(&[0xFE, 0xFF, 0x00, 0x41, 0x00]),
            "A\u{FFFD}"
        );
        assert_eq!(decode_pdf_string(&[0xFE, 0xFF, 0xD8, 0x3D]), "\u{FFFD}");
    }

    #[test]
    fn utf8_with_bom() {
        assert_eq!(decode_pdf_string(b"\xEF\xBB\xBFcaf\xC3\xA9"), "café");
    }

    #[test]
    fn pdfdoc_encoding_differs_from_latin1_in_the_high_range() {
        assert_eq!(decode_pdf_string(&[0x80]), "\u{2022}");
        assert_eq!(decode_pdf_string(&[0x84, 0x85]), "\u{2014}\u{2013}");
        assert_eq!(decode_pdf_string(&[0x8D, 0x41, 0x8E]), "\u{201C}A\u{201D}");
        assert_eq!(decode_pdf_string(&[0x92, 0x93]), "\u{2122}\u{FB01}");
        assert_eq!(decode_pdf_string(&[0x9E]), "\u{017E}");
        assert_eq!(decode_pdf_string(&[0xA0]), "\u{20AC}");
        assert_eq!(decode_pdf_string(&[0x18, 0x1F]), "\u{02D8}\u{02DC}");
    }

    #[test]
    fn pdfdoc_upper_half_is_latin1() {
        assert_eq!(decode_pdf_string(b"Jos\xE9 \xD1and\xFA"), "José Ñandú");
    }

    #[test]
    fn undefined_codes_become_replacement() {
        assert_eq!(decode_pdf_string(&[0x9F]), "\u{FFFD}");
        assert_eq!(decode_pdf_string(&[0x7F]), "\u{FFFD}");
    }
}

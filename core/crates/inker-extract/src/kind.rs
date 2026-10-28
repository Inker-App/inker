//! File kind detection: magic bytes first, extension second.

use std::{fs::File, io::Read};

use camino::Utf8Path;

/// What kind of file the pipeline is looking at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    Pdf,
    /// PNG, JPEG or HEIC.
    Image,
    /// Plain text, Markdown or CSV.
    Text,
    Unsupported,
}

/// How much of the file to look at.
const SNIFF_LEN: usize = 8192;
/// PDF readers accept the header anywhere in the first 1024 bytes.
const PDF_HEADER_WINDOW: usize = 1024;
/// ISO-BMFF brands used by HEIC/HEIF stills and sequences.
const HEIF_BRANDS: [&[u8; 4]; 8] = [
    b"heic", b"heix", b"hevc", b"hevx", b"heim", b"heis", b"mif1", b"msf1",
];

pub(crate) fn is_pdf(head: &[u8]) -> bool {
    head[..head.len().min(PDF_HEADER_WINDOW)]
        .windows(5)
        .any(|w| w == b"%PDF-")
}

fn is_image(head: &[u8]) -> bool {
    let png = head.starts_with(b"\x89PNG\r\n\x1a\n");
    let jpeg = head.starts_with(&[0xFF, 0xD8, 0xFF]);
    let heif = head.len() >= 12
        && &head[4..8] == b"ftyp"
        && HEIF_BRANDS.iter().any(|b| &head[8..12] == *b);
    png || jpeg || heif
}

fn extension(path: &Utf8Path) -> String {
    path.extension().unwrap_or_default().to_ascii_lowercase()
}

fn read_head(path: &Utf8Path) -> std::io::Result<Vec<u8>> {
    let mut head = Vec::with_capacity(SNIFF_LEN);
    File::open(path)?
        .take(SNIFF_LEN as u64)
        .read_to_end(&mut head)?;
    Ok(head)
}

/// Classify a file. Magic bytes win over the extension, and an extension
/// that claims PDF or image without the magic is not trusted. Text is
/// recognised by extension (txt, md, csv) and must not contain NUL bytes. A
/// file that cannot be read is classified by extension alone.
pub fn detect_file_kind(path: &Utf8Path) -> FileKind {
    let ext = extension(path);
    let is_text_ext = matches!(ext.as_str(), "txt" | "md" | "csv");
    match read_head(path) {
        Ok(head) if is_pdf(&head) => FileKind::Pdf,
        Ok(head) if is_image(&head) => FileKind::Image,
        Ok(head) if is_text_ext && !head.contains(&0) => FileKind::Text,
        Ok(_) => FileKind::Unsupported,
        Err(_) => match ext.as_str() {
            "pdf" => FileKind::Pdf,
            "png" | "jpg" | "jpeg" | "heic" => FileKind::Image,
            _ if is_text_ext => FileKind::Text,
            _ => FileKind::Unsupported,
        },
    }
}

//! Finding documents in the popover: the ready board grouped by year, tag
//! counts for the chips, and matching for the Find field. Pure functions over
//! records; `Core::board` and `Core::find` feed them from the index and the
//! vault. Nothing here reads a file.
//!
//! The payer is the one real value that reaches the shell: the employer,
//! payer or lender a template attributed, read back through the vault for the
//! popover only. It never goes over MCP and never into a log.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};

use crate::types::{DocKind, DocumentRecord, FolderRecord, FolderSummary, HandleStyle};

/// A document as the board shows it.
#[derive(Debug, Clone, PartialEq)]
pub struct DocView {
    pub record: DocumentRecord,
    /// "Acme Robotics": the template's employer, payer or lender, if any.
    pub payer: Option<String>,
    /// Original name of the folder it was dropped in, if any.
    pub folder_name: Option<String>,
}

/// A dropped folder: a sub-group inside its year.
#[derive(Debug, Clone, PartialEq)]
pub struct FolderView {
    pub record: FolderRecord,
    /// The folder's original name, from its path.
    pub name: String,
    pub summary: FolderSummary,
    /// Newest drop first.
    pub children: Vec<DocView>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum BoardEntry {
    Doc(DocView),
    Folder(FolderView),
}

impl BoardEntry {
    /// When the entry was dropped: a folder counts as its newest child.
    fn dropped_at(&self) -> Option<DateTime<Utc>> {
        match self {
            BoardEntry::Doc(d) => Some(d.record.created_at),
            BoardEntry::Folder(f) => f.children.iter().map(|c| c.record.created_at).max(),
        }
    }

    fn files(&self) -> u32 {
        match self {
            BoardEntry::Doc(_) => 1,
            BoardEntry::Folder(f) => u32::try_from(f.children.len()).unwrap_or(u32::MAX),
        }
    }
}

/// `2025 · 14 FILES`; `year` is `None` for `UNDATED`.
#[derive(Debug, Clone, PartialEq)]
pub struct YearGroup {
    pub year: Option<u16>,
    pub files: u32,
    pub entries: Vec<BoardEntry>,
}

/// `Taxes · 24` on a chip.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagCount {
    pub tag: String,
    pub count: u32,
}

/// The ready board at any size.
#[derive(Debug, Clone, PartialEq)]
pub struct Board {
    /// Newest year first, `UNDATED` last.
    pub groups: Vec<YearGroup>,
    /// Every tag with at least one file, most files first, then by name.
    pub tags: Vec<TagCount>,
    /// Every document, whatever its state.
    pub total: u32,
}

/// The expanded row: where the file is, when it came, who read it.
#[derive(Debug, Clone, PartialEq)]
pub struct DocDetails {
    /// The folder the file sits in, relative to the private folder, with a
    /// trailing slash: `Taxes 2025/`, or empty at the root.
    pub location: String,
    pub dropped_at: DateTime<Utc>,
    /// Files that came in the same folder drop, not counting this one.
    pub dropped_with: u32,
    pub last_read: Option<crate::types::AgentRead>,
}

/// Which field a Find query matched, in ranking order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MatchField {
    FileName,
    Handle,
    Kind,
    Payer,
    Year,
    Folder,
    Tag,
    Text,
}

/// One Find result.
#[derive(Debug, Clone, PartialEq)]
pub struct Match {
    pub doc: DocView,
    pub field: MatchField,
    /// The matched characters as they appear in that field, for underlining.
    pub matched: String,
    /// For a text match: the line around the hit, trimmed to a window.
    pub snippet: Option<String>,
    /// The query was this document's whole handle (with or without a prefix).
    pub exact_handle: bool,
}

/// A folder's original name: the last path component.
pub fn folder_name(f: &FolderRecord) -> String {
    f.path
        .file_name()
        .map(str::to_string)
        .unwrap_or_else(|| f.codename.as_str().to_string())
}

/// Group documents by tax year, newest year first and `UNDATED` last. Inside
/// a year, newest drop first; a dropped folder is one entry placed by its
/// newest child and in the year most of its children have (ties to the
/// newest). Documents in a folder that is not in `folders` are listed loose.
pub fn group(docs: Vec<DocView>, folders: &[FolderRecord]) -> Board {
    let total = u32::try_from(docs.len()).unwrap_or(u32::MAX);
    let tags = tag_counts(&docs);

    let mut loose: Vec<DocView> = Vec::new();
    let mut by_folder: BTreeMap<String, Vec<DocView>> = BTreeMap::new();
    for d in docs {
        match d.record.folder {
            Some(id) if folders.iter().any(|f| f.id == id) => {
                by_folder.entry(id.0.to_string()).or_default().push(d);
            }
            _ => loose.push(d),
        }
    }

    let mut entries: Vec<(Option<u16>, BoardEntry)> = loose
        .into_iter()
        .map(|d| (d.record.year, BoardEntry::Doc(d)))
        .collect();
    for f in folders {
        let Some(mut children) = by_folder.remove(&f.id.0.to_string()) else {
            continue;
        };
        children.sort_by_key(|c| std::cmp::Reverse(c.record.created_at));
        let year = folder_year(&children);
        let summary = summary_of(&children);
        entries.push((
            year,
            BoardEntry::Folder(FolderView {
                record: f.clone(),
                name: folder_name(f),
                summary,
                children,
            }),
        ));
    }

    let mut groups: BTreeMap<Option<u16>, Vec<BoardEntry>> = BTreeMap::new();
    for (year, e) in entries {
        groups.entry(year).or_default().push(e);
    }
    let mut out: Vec<YearGroup> = groups
        .into_iter()
        .map(|(year, mut entries)| {
            entries.sort_by_key(|e| std::cmp::Reverse(e.dropped_at()));
            YearGroup {
                year,
                files: entries.iter().map(BoardEntry::files).sum(),
                entries,
            }
        })
        .collect();
    // Newest year first; None (undated) last.
    out.sort_by(|a, b| match (a.year, b.year) {
        (Some(x), Some(y)) => y.cmp(&x),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    });
    Board {
        groups: out,
        tags,
        total,
    }
}

/// The documents carrying `tag`, for a chip; `None` is `All`.
pub fn with_tag(docs: Vec<DocView>, tag: Option<&str>) -> Vec<DocView> {
    match tag {
        None => docs,
        Some(t) => docs
            .into_iter()
            .filter(|d| d.record.tag.as_deref() == Some(t))
            .collect(),
    }
}

fn tag_counts(docs: &[DocView]) -> Vec<TagCount> {
    let mut counts: BTreeMap<&str, u32> = BTreeMap::new();
    for d in docs {
        if let Some(t) = d.record.tag.as_deref() {
            *counts.entry(t).or_default() += 1;
        }
    }
    let mut out: Vec<TagCount> = counts
        .into_iter()
        .map(|(tag, count)| TagCount {
            tag: tag.to_string(),
            count,
        })
        .collect();
    out.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.tag.cmp(&b.tag)));
    out
}

/// The year most children have; ties go to the newest drop among them.
fn folder_year(children: &[DocView]) -> Option<u16> {
    let mut counts: BTreeMap<u16, (usize, DateTime<Utc>)> = BTreeMap::new();
    for c in children {
        if let Some(y) = c.record.year {
            let e = counts.entry(y).or_insert((0, c.record.created_at));
            e.0 += 1;
            e.1 = e.1.max(c.record.created_at);
        }
    }
    counts
        .into_iter()
        .max_by(|a, b| a.1.cmp(&b.1))
        .map(|(y, _)| y)
}

fn summary_of(children: &[DocView]) -> FolderSummary {
    let mut s = FolderSummary::default();
    for c in children {
        s.total += 1;
        s.inked += c.record.inked;
        match c.record.state {
            crate::types::DocState::Ready => s.ready += 1,
            crate::types::DocState::NeedsAttention => s.needs_attention += 1,
            crate::types::DocState::Failed => s.failed += 1,
            crate::types::DocState::Queued
            | crate::types::DocState::Reading
            | crate::types::DocState::Inking => s.inking += 1,
        }
    }
    s
}

// ---------------------------------------------------------------------------
// Find
// ---------------------------------------------------------------------------

/// Characters of context kept on each side of a text hit.
const SNIPPET_CONTEXT: usize = 40;

/// The query with a handle prefix (`🐙/` or `ink/`) removed and whitespace
/// trimmed. Empty when there is nothing to search for.
pub fn normalize_query(q: &str) -> String {
    let t = q.trim();
    for style in [HandleStyle::Octopus, HandleStyle::Ascii] {
        if let Some(rest) = t.strip_prefix(style.prefix()) {
            return rest.trim().to_string();
        }
    }
    t.to_string()
}

/// Case-insensitive substring search; the matched text as it appears in
/// `hay`, with its byte range.
fn find_in(hay: &str, needle_lower: &str) -> Option<(usize, usize)> {
    if needle_lower.is_empty() {
        return None;
    }
    let lower = hay.to_lowercase();
    // `to_lowercase` can change byte lengths (e.g. İ), so map through chars.
    if lower.len() == hay.len() {
        let start = lower.find(needle_lower)?;
        return Some((start, start + needle_lower.len()));
    }
    let lchars: Vec<(usize, char)> = hay.char_indices().collect();
    let folded: Vec<String> = lchars
        .iter()
        .map(|(_, c)| c.to_lowercase().collect())
        .collect();
    for i in 0..folded.len() {
        let mut acc = String::new();
        let mut j = i;
        while acc.len() < needle_lower.len() && j < folded.len() {
            acc.push_str(&folded[j]);
            j += 1;
        }
        if acc == needle_lower {
            let end = lchars.get(j).map_or(hay.len(), |(b, _)| *b);
            return Some((lchars[i].0, end));
        }
    }
    None
}

/// Match one document's own fields, best field first. `kind_label` is the
/// row's form type ("W-2"), which the query is matched against as well as
/// the raw kind.
pub fn match_fields(doc: &DocView, kind_label: &str, query: &str) -> Option<Match> {
    let q = normalize_query(query).to_lowercase();
    if q.is_empty() {
        return None;
    }
    let r = &doc.record;
    let hit = |field: MatchField, hay: &str| -> Option<Match> {
        find_in(hay, &q).map(|(s, e)| Match {
            doc: doc.clone(),
            field,
            matched: hay[s..e].to_string(),
            snippet: None,
            exact_handle: field == MatchField::Handle && hay.len() == q.len(),
        })
    };
    hit(MatchField::FileName, &r.original_name)
        .or_else(|| hit(MatchField::Handle, r.codename.as_str()))
        .or_else(|| hit(MatchField::Kind, kind_label))
        .or_else(|| hit(MatchField::Kind, r.kind.as_str()))
        .or_else(|| doc.payer.as_deref().and_then(|p| hit(MatchField::Payer, p)))
        .or_else(|| r.year.and_then(|y| hit(MatchField::Year, &y.to_string())))
        .or_else(|| {
            doc.folder_name
                .as_deref()
                .and_then(|f| hit(MatchField::Folder, f))
        })
        .or_else(|| r.tag.as_deref().and_then(|t| hit(MatchField::Tag, t)))
}

/// A text hit: the line holding the first match, trimmed to a window around
/// it with an ellipsis on each cut side, and the matched characters.
pub fn text_snippet(page_text: &str, query: &str) -> Option<(String, String)> {
    let q = normalize_query(query).to_lowercase();
    if q.is_empty() {
        return None;
    }
    for line in page_text.lines() {
        let line = line.trim();
        if let Some((s, e)) = find_in(line, &q) {
            let matched = line[s..e].to_string();
            let start = line[..s]
                .char_indices()
                .rev()
                .nth(SNIPPET_CONTEXT - 1)
                .map_or(0, |(i, _)| i);
            let end = line[e..]
                .char_indices()
                .nth(SNIPPET_CONTEXT)
                .map_or(line.len(), |(i, _)| e + i);
            let mut out = String::new();
            if start > 0 {
                out.push('…');
            }
            out.push_str(line[start..end].trim());
            if end < line.len() {
                out.push('…');
            }
            return Some((out, matched));
        }
    }
    None
}

/// Order results by field, then newest drop first. Each document appears
/// once: the caller passes one match per document.
pub fn rank(mut matches: Vec<Match>) -> Vec<Match> {
    matches.sort_by(|a, b| {
        a.field
            .cmp(&b.field)
            .then_with(|| b.doc.record.created_at.cmp(&a.doc.record.created_at))
            .then_with(|| {
                a.doc
                    .record
                    .codename
                    .as_str()
                    .cmp(b.doc.record.codename.as_str())
            })
    });
    matches
}

/// The row's form type: "W-2", "1099-INT", "K-1", "bank statement".
pub fn kind_label(kind: &DocKind) -> String {
    match kind.as_str() {
        "" | DocKind::UNKNOWN => "document".into(),
        "w2" => "W-2".into(),
        k if k.starts_with("k1-") || k == "k1" => "K-1".into(),
        k if k.starts_with("109") => k.to_ascii_uppercase(),
        k => k.replace('-', " "),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Codename, DocState, DocumentId, FolderId, IngestMode};
    use camino::Utf8PathBuf;
    use chrono::TimeZone;

    fn at(day: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 1, day, 12, 0, 0).unwrap()
    }

    #[allow(clippy::too_many_arguments)]
    fn doc(
        code: &str,
        name: &str,
        kind: &str,
        year: Option<u16>,
        day: u32,
        folder: Option<&FolderRecord>,
        tag: Option<&str>,
        payer: Option<&str>,
    ) -> DocView {
        DocView {
            record: DocumentRecord {
                id: DocumentId::new(),
                codename: Codename::parse(code).unwrap(),
                path: Utf8PathBuf::from(format!("/Users/t/Private/{name}")),
                original_name: name.into(),
                kind: DocKind(kind.into()),
                year,
                state: DocState::Ready,
                inked: 6,
                folder: folder.map(|f| f.id),
                ingest: IngestMode::Move,
                created_at: at(day),
                updated_at: at(day),
                status_note: None,
                tag: tag.map(str::to_string),
                payer_token: None,
            },
            payer: payer.map(str::to_string),
            folder_name: folder.map(folder_name),
        }
    }

    fn folder(code: &str, name: &str, day: u32) -> FolderRecord {
        FolderRecord {
            id: FolderId::new(),
            codename: Codename::parse(code).unwrap(),
            path: Utf8PathBuf::from(format!("/Users/t/Private/{name}")),
            created_at: at(day),
        }
    }

    /// 30 files: 2025 has a 9-file folder and 4 loose, 2024 a 6-file folder
    /// and 4 loose, 2023 has 5 loose, and 2 are undated.
    fn thirty() -> (Vec<DocView>, Vec<FolderRecord>) {
        let f25 = folder("folder-0412", "Taxes 2025", 14);
        let f24 = folder("folder-0377", "Taxes 2024", 3);
        let mut docs = Vec::new();
        for i in 0..9 {
            let code = if i == 0 {
                "w2-2025".to_string()
            } else {
                format!("w2-2025-{}", i + 1)
            };
            docs.push(doc(
                &code,
                &format!("w2-{i}.pdf"),
                "w2",
                Some(2025),
                14,
                Some(&f25),
                Some("Taxes"),
                Some("Acme Robotics"),
            ));
        }
        docs.push(doc(
            "1099-int-2025",
            "IMG_4471.heic",
            "1099-int",
            Some(2025),
            20,
            None,
            Some("Taxes"),
            Some("Harbor Street CU"),
        ));
        docs.push(doc(
            "1098-2025",
            "1098_harbor.pdf",
            "1098",
            Some(2025),
            18,
            None,
            Some("Taxes"),
            Some("Harbor Street CU"),
        ));
        docs.push(doc(
            "bank-statement-2025",
            "statement-march.pdf",
            "bank-statement",
            Some(2025),
            16,
            None,
            Some("Banking"),
            None,
        ));
        docs.push(doc(
            "closing-statement-2025",
            "closing.pdf",
            "closing-statement",
            Some(2025),
            15,
            None,
            Some("Legal"),
            None,
        ));
        for i in 0..6 {
            docs.push(doc(
                &format!("1099-nec-2024-{}", i + 2),
                &format!("nec-{i}.pdf"),
                "1099-nec",
                Some(2024),
                3,
                Some(&f24),
                Some("Taxes"),
                Some("Bluefin Analytics"),
            ));
        }
        for i in 0..4 {
            docs.push(doc(
                &format!("w2-2024-{}", i + 2),
                &format!("w2-2024-{i}.pdf"),
                "w2",
                Some(2024),
                5 + i,
                None,
                Some("Taxes"),
                Some("Acme Robotics"),
            ));
        }
        for i in 0..5 {
            docs.push(doc(
                &format!("1040-2023-{}", i + 2),
                &format!("1040-{i}.pdf"),
                "1040",
                Some(2023),
                1,
                None,
                Some("Taxes"),
                None,
            ));
        }
        docs.push(doc(
            "receipt-4471",
            "IMG_0001.heic",
            "receipt",
            None,
            2,
            None,
            None,
            None,
        ));
        docs.push(doc(
            "scan-4472",
            "IMG_0002.heic",
            "scan",
            None,
            1,
            None,
            None,
            None,
        ));
        assert_eq!(docs.len(), 30);
        (docs, vec![f25, f24])
    }

    #[test]
    fn thirty_files_group_by_year_newest_first_with_folders_and_undated() {
        let (docs, folders) = thirty();
        let b = group(docs, &folders);
        assert_eq!(b.total, 30);
        let years: Vec<Option<u16>> = b.groups.iter().map(|g| g.year).collect();
        assert_eq!(years, vec![Some(2025), Some(2024), Some(2023), None]);
        let files: Vec<u32> = b.groups.iter().map(|g| g.files).collect();
        assert_eq!(files, vec![13, 10, 5, 2]);

        // 2025: newest drop first; the folder sits where its newest child does.
        let g = &b.groups[0];
        let order: Vec<String> = g
            .entries
            .iter()
            .map(|e| match e {
                BoardEntry::Doc(d) => d.record.codename.as_str().to_string(),
                BoardEntry::Folder(f) => format!("folder:{}", f.name),
            })
            .collect();
        assert_eq!(
            order,
            vec![
                "1099-int-2025",
                "1098-2025",
                "bank-statement-2025",
                "closing-statement-2025",
                "folder:Taxes 2025"
            ]
        );
        let BoardEntry::Folder(f) = &g.entries[4] else {
            panic!("folder")
        };
        assert_eq!(f.record.codename.as_str(), "folder-0412");
        assert_eq!(f.children.len(), 9);
        assert_eq!(f.summary.total, 9);
        assert_eq!(f.summary.ready, 9);
        assert_eq!(f.summary.inked, 54);

        // 2024: loose files newest first, then the older folder.
        let g = &b.groups[1];
        assert!(
            matches!(&g.entries[0], BoardEntry::Doc(d) if d.record.original_name == "w2-2024-3.pdf")
        );
        assert!(matches!(g.entries.last(), Some(BoardEntry::Folder(f)) if f.name == "Taxes 2024"));

        // Undated last, newest first.
        let g = &b.groups[3];
        assert_eq!(g.year, None);
        assert!(
            matches!(&g.entries[0], BoardEntry::Doc(d) if d.record.codename.as_str() == "receipt-4471")
        );
    }

    #[test]
    fn tag_counts_are_most_files_first_and_skip_untagged() {
        let (docs, folders) = thirty();
        let b = group(docs, &folders);
        assert_eq!(
            b.tags,
            vec![
                TagCount {
                    tag: "Taxes".into(),
                    count: 26
                },
                TagCount {
                    tag: "Banking".into(),
                    count: 1
                },
                TagCount {
                    tag: "Legal".into(),
                    count: 1
                },
            ]
        );
    }

    #[test]
    fn a_tag_chip_filters_before_grouping_and_years_still_apply() {
        let (docs, folders) = thirty();
        let b = group(with_tag(docs, Some("Taxes")), &folders);
        assert_eq!(b.total, 26);
        let files: Vec<u32> = b.groups.iter().map(|g| g.files).collect();
        assert_eq!(files, vec![11, 10, 5]);
        assert!(
            b.groups.iter().all(|g| g.year.is_some()),
            "no undated under Taxes"
        );
    }

    #[test]
    fn a_folder_with_mixed_years_goes_where_most_of_its_files_are() {
        let f = folder("folder-0001", "Mixed", 1);
        let docs = vec![
            doc(
                "w2-2025",
                "a.pdf",
                "w2",
                Some(2025),
                1,
                Some(&f),
                None,
                None,
            ),
            doc(
                "w2-2024",
                "b.pdf",
                "w2",
                Some(2024),
                2,
                Some(&f),
                None,
                None,
            ),
            doc(
                "w2-2024-2",
                "c.pdf",
                "w2",
                Some(2024),
                3,
                Some(&f),
                None,
                None,
            ),
            doc("scan-1", "d.heic", "scan", None, 4, Some(&f), None, None),
        ];
        let b = group(docs, &[f]);
        assert_eq!(b.groups.len(), 1);
        assert_eq!(b.groups[0].year, Some(2024));
        assert_eq!(b.groups[0].files, 4);
    }

    #[test]
    fn a_folder_of_undated_files_is_undated() {
        let f = folder("folder-0001", "Scans", 1);
        let docs = vec![
            doc("scan-1", "a.heic", "scan", None, 1, Some(&f), None, None),
            doc("scan-2", "b.heic", "scan", None, 2, Some(&f), None, None),
        ];
        let b = group(docs, &[f]);
        assert_eq!(b.groups[0].year, None);
    }

    #[test]
    fn a_document_whose_folder_is_gone_is_listed_loose() {
        let f = folder("folder-0001", "Gone", 1);
        let docs = vec![doc(
            "w2-2025",
            "a.pdf",
            "w2",
            Some(2025),
            1,
            Some(&f),
            None,
            None,
        )];
        let b = group(docs, &[]);
        assert!(matches!(&b.groups[0].entries[0], BoardEntry::Doc(_)));
    }

    // --- find -----------------------------------------------------------

    fn one() -> DocView {
        let f = folder("folder-0412", "Taxes 2025", 14);
        doc(
            "w2-2025-2",
            "w2-bluefin.pdf",
            "w2",
            Some(2025),
            14,
            Some(&f),
            Some("Taxes"),
            Some("Bluefin Analytics"),
        )
    }

    fn field_of(q: &str) -> Option<(MatchField, String, bool)> {
        match_fields(&one(), "W-2", q).map(|m| (m.field, m.matched, m.exact_handle))
    }

    #[test]
    fn every_field_matches_case_insensitively_in_ranking_order() {
        assert_eq!(
            field_of("BLUEFIN.pdf"),
            Some((MatchField::FileName, "bluefin.pdf".into(), false))
        );
        assert_eq!(
            field_of("2025-2"),
            Some((MatchField::Handle, "2025-2".into(), false))
        );
        assert_eq!(
            field_of("w-2"),
            Some((MatchField::Kind, "W-2".into(), false))
        );
        assert_eq!(
            field_of("Analytics"),
            Some((MatchField::Payer, "Analytics".into(), false))
        );
        assert_eq!(
            field_of("Taxes 20"),
            Some((MatchField::Folder, "Taxes 20".into(), false))
        );
        assert_eq!(
            field_of("taxes"),
            Some((MatchField::Folder, "Taxes".into(), false)),
            "folder name wins over tag"
        );
        let f = folder("folder-0412", "Receipts", 14);
        let d = doc(
            "w2-2025-2",
            "x.pdf",
            "w2",
            Some(2025),
            14,
            Some(&f),
            Some("Taxes"),
            None,
        );
        assert_eq!(
            match_fields(&d, "W-2", "tax").map(|m| m.field),
            Some(MatchField::Tag)
        );
        assert_eq!(
            match_fields(&d, "W-2", "2025").map(|m| m.field),
            Some(MatchField::Handle),
            "the year is in the handle first"
        );
        let d = doc(
            "receipt-4471",
            "x.pdf",
            "receipt",
            Some(2025),
            14,
            None,
            None,
            None,
        );
        assert_eq!(
            match_fields(&d, "receipt", "2025").map(|m| m.field),
            Some(MatchField::Year)
        );
        assert_eq!(field_of("nothing here"), None);
        assert_eq!(field_of("   "), None);
    }

    #[test]
    fn a_handle_query_matches_with_or_without_its_prefix_and_knows_when_it_is_exact() {
        assert_eq!(
            field_of("🐙/w2-2025-2"),
            Some((MatchField::Handle, "w2-2025-2".into(), true))
        );
        assert_eq!(
            field_of("ink/w2-2025-2"),
            Some((MatchField::Handle, "w2-2025-2".into(), true))
        );
        assert_eq!(
            field_of("W2-2025-2"),
            Some((MatchField::Handle, "w2-2025-2".into(), true))
        );
        assert_eq!(
            field_of("🐙/w2-2025"),
            Some((MatchField::Handle, "w2-2025".into(), false))
        );
        assert_eq!(normalize_query("  🐙/ w2-2025 "), "w2-2025");
        assert_eq!(normalize_query("ink/"), "");
    }

    #[test]
    fn a_text_hit_returns_the_line_trimmed_around_the_match() {
        let page = "Form 1098\nMortgage Interest Statement for [PERSON_1]\n1 Mortgage interest received from payer(s)/borrower(s) 12,345.00";
        let (snippet, matched) = text_snippet(page, "mortgage interest").unwrap();
        assert_eq!(matched, "Mortgage Interest");
        assert_eq!(snippet, "Mortgage Interest Statement for [PERSON_1]");

        let long = format!(
            "{} Mortgage interest received {}",
            "x".repeat(60),
            "y".repeat(60)
        );
        let (snippet, _) = text_snippet(&long, "interest received").unwrap();
        assert!(
            snippet.starts_with('…') && snippet.ends_with('…'),
            "{snippet}"
        );
        assert!(snippet.contains("Mortgage interest received"));
        assert!(snippet.chars().count() < 120);
        assert_eq!(text_snippet(page, "zzz"), None);
    }

    #[test]
    fn ranking_is_by_field_then_newest_drop() {
        let old = doc(
            "w2-2024",
            "acme-2024.pdf",
            "w2",
            Some(2024),
            1,
            None,
            None,
            Some("Acme"),
        );
        let new = doc(
            "w2-2025",
            "acme-2025.pdf",
            "w2",
            Some(2025),
            9,
            None,
            None,
            Some("Acme"),
        );
        let payer_only = doc(
            "1099-int-2025",
            "harbor.pdf",
            "1099-int",
            Some(2025),
            20,
            None,
            None,
            Some("Acme Bank"),
        );
        let ms: Vec<Match> = [&old, &new, &payer_only]
            .into_iter()
            .filter_map(|d| match_fields(d, &kind_label(&d.record.kind), "acme"))
            .collect();
        let ranked: Vec<String> = rank(ms)
            .iter()
            .map(|m| m.doc.record.codename.as_str().to_string())
            .collect();
        assert_eq!(ranked, vec!["w2-2025", "w2-2024", "1099-int-2025"]);
    }

    #[test]
    fn kind_labels_read_like_the_boards() {
        assert_eq!(kind_label(&DocKind("w2".into())), "W-2");
        assert_eq!(kind_label(&DocKind("1099-int".into())), "1099-INT");
        assert_eq!(kind_label(&DocKind("k1-1065".into())), "K-1");
        assert_eq!(
            kind_label(&DocKind("bank-statement".into())),
            "bank statement"
        );
        assert_eq!(kind_label(&DocKind::unknown()), "document");
    }
}

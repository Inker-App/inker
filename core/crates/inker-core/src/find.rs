//! Finding documents in the popover: the ready board as a tree that mirrors
//! the private folder on disk, and matching for the Find field. Pure
//! functions over records; `Core::board` and `Core::find` feed them from the
//! index and the vault. Nothing here reads a file.
//!
//! The payer is the one real value that reaches the shell: the employer,
//! payer or lender a template attributed, read back through the vault for the
//! popover only. It never goes over MCP and never into a log.

use std::cmp::Ordering;

use camino::Utf8Path;
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

/// One folder of the board's tree. Generic so the shell's preview data can
/// be laid out by the same rules as the core's records.
#[derive(Debug, Clone, PartialEq)]
pub struct Tree<D, F> {
    /// The folder's name on disk; empty for the root.
    pub name: String,
    /// Relative to the private folder, `/`-separated; empty for the root.
    pub path: String,
    /// The dropped folder at this path, if one was dropped here.
    pub folder: Option<F>,
    /// Sub-folders, in Finder order.
    pub folders: Vec<Tree<D, F>>,
    /// Files directly in this folder, in Finder order.
    pub files: Vec<D>,
}

impl<D, F> Tree<D, F> {
    fn new(name: &str, path: String) -> Self {
        Self {
            name: name.to_string(),
            path,
            folder: None,
            folders: Vec::new(),
            files: Vec::new(),
        }
    }

    /// Every file under this folder, at any depth.
    pub fn file_count(&self) -> u32 {
        let own = u32::try_from(self.files.len()).unwrap_or(u32::MAX);
        self.folders
            .iter()
            .fold(own, |n, f| n.saturating_add(f.file_count()))
    }

    /// Every file under this folder, depth first.
    pub fn all_files(&self) -> Vec<&D> {
        let mut out: Vec<&D> = Vec::new();
        for f in &self.folders {
            out.extend(f.all_files());
        }
        out.extend(self.files.iter());
        out
    }

    /// The folder at `segments` below this one, created if missing.
    fn descend(&mut self, segments: &[&str]) -> &mut Self {
        let Some((first, rest)) = segments.split_first() else {
            return self;
        };
        let i = match self.folders.iter().position(|f| f.name == *first) {
            Some(i) => i,
            None => {
                let path = if self.path.is_empty() {
                    (*first).to_string()
                } else {
                    format!("{}/{first}", self.path)
                };
                self.folders.push(Tree::new(first, path));
                self.folders.len() - 1
            }
        };
        self.folders[i].descend(rest)
    }

    /// The folder at `segments`, if it exists.
    fn find_mut(&mut self, segments: &[&str]) -> Option<&mut Self> {
        match segments.split_first() {
            None => Some(self),
            Some((first, rest)) => self
                .folders
                .iter_mut()
                .find(|f| f.name == *first)?
                .find_mut(rest),
        }
    }

    fn sort(&mut self, name_of: &impl Fn(&D) -> &str) {
        self.folders.sort_by(|a, b| finder_order(&a.name, &b.name));
        self.files
            .sort_by(|a, b| finder_order(name_of(a), name_of(b)));
        for f in &mut self.folders {
            f.sort(name_of);
        }
    }
}

fn segments(rel: &str) -> Vec<&str> {
    rel.split('/').filter(|s| !s.is_empty()).collect()
}

/// Lay files out by the folder each sits in. `files` pairs each file with
/// its folder relative to the root (empty for the root itself); `folders`
/// pairs each dropped folder with its path, and a folder no file sits under
/// is left out. Sub-folders come first, then files, each in Finder order by
/// name (`name_of` for a file).
pub fn build_tree<D, F>(
    files: Vec<(String, D)>,
    folders: Vec<(String, F)>,
    name_of: impl Fn(&D) -> &str,
) -> Tree<D, F> {
    let mut root = Tree::new("", String::new());
    for (dir, file) in files {
        root.descend(&segments(&dir)).files.push(file);
    }
    for (dir, folder) in folders {
        if let Some(node) = root.find_mut(&segments(&dir)) {
            node.folder = Some(folder);
        }
    }
    root.sort(&name_of);
    root
}

/// A folder of the ready board, with the dropped folder at its path, if any.
pub type FolderNode = Tree<DocView, FolderRecord>;

impl FolderNode {
    /// "9 files · all ready · 41 inked" for everything under this folder.
    pub fn summary(&self) -> FolderSummary {
        summary_of(self.all_files())
    }
}

/// The ready board: the private folder as it is on disk.
#[derive(Debug, Clone, PartialEq)]
pub struct Board {
    /// The private folder itself; its name and path are empty.
    pub root: FolderNode,
    /// Every document, whatever its state.
    pub total: u32,
}

/// Lay documents out as the tree of `private_folder`. A document outside
/// it (which ingest does not make) is listed at the root.
pub fn tree(docs: Vec<DocView>, folders: &[FolderRecord], private_folder: &Utf8Path) -> Board {
    let total = u32::try_from(docs.len()).unwrap_or(u32::MAX);
    let rel = |p: &Utf8Path| -> Option<String> {
        p.strip_prefix(private_folder)
            .ok()
            .map(|r| r.as_str().to_string())
    };
    let files = docs
        .into_iter()
        .map(|d| {
            let dir = d.record.path.parent().and_then(rel).unwrap_or_default();
            (dir, d)
        })
        .collect();
    let dropped = folders
        .iter()
        .filter_map(|f| rel(&f.path).map(|r| (r, f.clone())))
        .collect();
    Board {
        root: build_tree(files, dropped, |d: &DocView| {
            d.record.original_name.as_str()
        }),
        total,
    }
}

/// Finder's order for names: case-insensitive, with runs of digits read as
/// numbers, so `9.pdf` comes before `10.pdf`. Ties fall back to the plain
/// text so the order is total.
pub fn finder_order(a: &str, b: &str) -> Ordering {
    let (mut x, mut y) = (a.chars().peekable(), b.chars().peekable());
    loop {
        let o = match (x.peek().copied(), y.peek().copied()) {
            (None, None) => break,
            (None, Some(_)) => Ordering::Less,
            (Some(_), None) => Ordering::Greater,
            (Some(cx), Some(cy)) if cx.is_ascii_digit() && cy.is_ascii_digit() => {
                let (nx, ny) = (digit_run(&mut x), digit_run(&mut y));
                let (tx, ty) = (nx.trim_start_matches('0'), ny.trim_start_matches('0'));
                tx.len().cmp(&ty.len()).then_with(|| tx.cmp(ty))
            }
            (Some(cx), Some(cy)) => {
                x.next();
                y.next();
                cx.to_lowercase().cmp(cy.to_lowercase())
            }
        };
        if o != Ordering::Equal {
            return o;
        }
    }
    a.len().cmp(&b.len()).then_with(|| a.cmp(b))
}

/// The digits at the front of `it`, consumed.
fn digit_run(it: &mut std::iter::Peekable<std::str::Chars<'_>>) -> String {
    let mut out = String::new();
    while let Some(c) = it.next_if(char::is_ascii_digit) {
        out.push(c);
    }
    out
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

fn summary_of<'a>(children: impl IntoIterator<Item = &'a DocView>) -> FolderSummary {
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
    use camino::{Utf8Path, Utf8PathBuf};
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

    /// A ready document at `rel` inside the private folder.
    fn at_path(code: &str, rel: &str, folder: Option<&FolderRecord>) -> DocView {
        let name = rel.rsplit('/').next().unwrap_or(rel);
        let mut d = doc(code, name, "w2", Some(2025), 1, folder, None, None);
        d.record.path = Utf8PathBuf::from(format!("/Users/t/Private/{rel}"));
        d
    }

    fn names<D, F>(t: &Tree<D, F>, name_of: impl Fn(&D) -> String) -> (Vec<String>, Vec<String>) {
        (
            t.folders.iter().map(|f| f.name.clone()).collect(),
            t.files.iter().map(name_of).collect(),
        )
    }

    fn doc_name(d: &DocView) -> String {
        d.record.original_name.clone()
    }

    #[test]
    fn finder_order_is_case_insensitive_and_reads_numbers_as_numbers() {
        let mut v = vec![
            "b.pdf", "A.pdf", "10.pdf", "9.pdf", "a2.pdf", "a10.pdf", "B.pdf",
        ];
        v.sort_by(|a, b| finder_order(a, b));
        assert_eq!(
            v,
            vec![
                "9.pdf", "10.pdf", "A.pdf", "a2.pdf", "a10.pdf", "B.pdf", "b.pdf"
            ]
        );
        assert_eq!(finder_order("x", "x"), std::cmp::Ordering::Equal);
        assert_eq!(finder_order("007", "7"), std::cmp::Ordering::Greater);
    }

    #[test]
    fn the_tree_mirrors_the_private_folder_folders_first_then_files_by_name() {
        let taxes = folder("folder-0412", "Taxes 2025", 1);
        let docs = vec![
            at_path("w2-2025", "b.pdf", None),
            at_path("w2-2025-2", "A.pdf", None),
            at_path("w2-2025-3", "10.pdf", None),
            at_path("w2-2025-4", "9.pdf", None),
            at_path("w2-2025-5", "Taxes 2025/w2.pdf", Some(&taxes)),
            at_path("w2-2025-6", "Taxes 2025/Receipts/R2.heic", Some(&taxes)),
            at_path("w2-2025-7", "Taxes 2025/Receipts/r1.heic", Some(&taxes)),
            at_path("w2-2025-8", "archive/old/x.pdf", None),
        ];
        let b = tree(docs, &[taxes], Utf8Path::new("/Users/t/Private"));
        assert_eq!(b.total, 8);
        let root = &b.root;
        assert_eq!(root.name, "");
        assert_eq!(root.path, "");
        assert!(root.folder.is_none());
        assert_eq!(root.file_count(), 8);
        assert_eq!(
            names(root, doc_name),
            (
                vec!["archive".to_string(), "Taxes 2025".into()],
                vec![
                    "9.pdf".to_string(),
                    "10.pdf".into(),
                    "A.pdf".into(),
                    "b.pdf".into()
                ]
            )
        );

        // A dropped folder carries its handle and summary.
        let t = &root.folders[1];
        assert_eq!(t.path, "Taxes 2025");
        assert_eq!(
            t.folder.as_ref().map(|f| f.codename.as_str()),
            Some("folder-0412")
        );
        assert_eq!(t.file_count(), 3);
        let s = t.summary();
        assert_eq!((s.total, s.ready, s.inked), (3, 3, 18));
        assert_eq!(
            names(t, doc_name),
            (vec!["Receipts".to_string()], vec!["w2.pdf".to_string()])
        );

        // A folder that is only a path segment has a name and a count, no handle.
        let r = &t.folders[0];
        assert_eq!(r.path, "Taxes 2025/Receipts");
        assert!(r.folder.is_none());
        assert_eq!(
            names(r, doc_name),
            (vec![], vec!["r1.heic".to_string(), "R2.heic".into()])
        );

        // Folders nest as deep as the paths go.
        let a = &root.folders[0];
        assert!(a.files.is_empty());
        assert_eq!(a.file_count(), 1);
        assert_eq!(a.folders[0].path, "archive/old");
        assert_eq!(a.folders[0].files[0].record.original_name, "x.pdf");
    }

    #[test]
    fn a_nested_dropped_folder_keeps_its_own_handle() {
        let outer = folder("folder-0001", "Taxes", 1);
        let inner = folder("folder-0002", "Taxes/2024", 2);
        let docs = vec![
            at_path("w2-2025", "Taxes/a.pdf", Some(&outer)),
            at_path("w2-2024", "Taxes/2024/b.pdf", Some(&inner)),
        ];
        let b = tree(docs, &[outer, inner], Utf8Path::new("/Users/t/Private"));
        let t = &b.root.folders[0];
        assert_eq!(
            t.folder.as_ref().map(|f| f.codename.as_str()),
            Some("folder-0001")
        );
        assert_eq!(t.summary().total, 2, "a summary covers everything under it");
        assert_eq!(
            t.folders[0].folder.as_ref().map(|f| f.codename.as_str()),
            Some("folder-0002")
        );
    }

    #[test]
    fn a_folder_without_files_is_left_out_and_a_stray_file_sits_at_the_root() {
        let empty = folder("folder-0001", "Empty", 1);
        let mut stray = at_path("w2-2025", "x.pdf", None);
        stray.record.path = Utf8PathBuf::from("/Users/t/Downloads/stray.pdf");
        stray.record.original_name = "stray.pdf".into();
        let b = tree(vec![stray], &[empty], Utf8Path::new("/Users/t/Private"));
        assert!(b.root.folders.is_empty());
        assert_eq!(names(&b.root, doc_name).1, vec!["stray.pdf".to_string()]);
    }

    #[test]
    fn an_empty_board_is_an_empty_root() {
        let b = tree(Vec::new(), &[], Utf8Path::new("/Users/t/Private"));
        assert_eq!(b.total, 0);
        assert!(b.root.folders.is_empty() && b.root.files.is_empty());
        assert_eq!(b.root.file_count(), 0);
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

//! An in-memory [`Backend`] for tests, the spike and `inker serve --fake`.
//!
//! All values are synthetic: the SSN uses area 9xx and the EIN an unissued
//! prefix, so nothing here can be a real person's identifier.

use std::collections::BTreeMap;
use std::sync::Mutex;

use inker_core::{
    Codename, Description, DocKind, DocState, FormField, Handle, KindTreatment, ListEntry,
    ResolvedDocument, ResolvedFolder, ResolvedPage, Token, TokenKind, TokenRole, Treatment,
};

use crate::backend::{Backend, Resolved, WriteReceipt};

/// The real values the fake pretends to hold in its vault. Tests assert no
/// server response ever contains any of these. All synthetic.
pub const FAKE_REAL_VALUES: &[&str] = &[
    "987-65-4320",
    "07-9876543",
    "Jordan Q. Sample",
    "Acme Fictional Widgets LLC",
    "100 Example Street",
];

/// Token label -> synthetic real value, the fake's "vault".
const VAULT: &[(&str, &str)] = &[
    ("[SSN_1]", "987-65-4320"),
    ("[EIN_1]", "07-9876543"),
    ("[PERSON_1]", "Jordan Q. Sample"),
    ("[ORG_1]", "Acme Fictional Widgets LLC"),
];

const W2_PAGE: &str = "Form W-2 Wage and Tax Statement 2025
a Employee's social security number: [SSN_1]
b Employer identification number (EIN): [EIN_1]
c Employer's name, address and ZIP code: [ORG_1], Springfield, IL
e Employee's name: [PERSON_1]
f Employee's address: Springfield, IL
1 Wages, tips, other compensation: 58,250.00
2 Federal income tax withheld: 6,420.00
3 Social security wages: 58,250.00
4 Social security tax withheld: 3,611.50
5 Medicare wages and tips: 58,250.00
6 Medicare tax withheld: 844.63
15 State: IL
16 State wages: 58,250.00
17 State income tax: 2,883.38";

#[derive(Debug, Clone)]
struct FakeDoc {
    entry: ListEntry,
    pages: Vec<String>,
    fields: Vec<FormField>,
    roles: Vec<(Token, &'static str)>,
    /// Children, for folders.
    children: Vec<String>,
}

/// In-memory backend.
#[derive(Debug)]
pub struct FakeBackend {
    docs: Mutex<BTreeMap<String, FakeDoc>>,
    order: Mutex<Vec<String>>,
    reads: Mutex<Vec<(String, Handle)>>,
    writes: Mutex<BTreeMap<String, String>>,
}

fn handle(codename: &str) -> Handle {
    // Codenames below are literals that satisfy the codename rules.
    Handle::new(Codename::parse(codename).unwrap_or_else(|_| unreachable!("bad literal codename")))
}

fn entry(codename: &str, kind: &str, state: DocState, inked: u32, is_folder: bool) -> ListEntry {
    ListEntry {
        handle: handle(codename),
        kind: DocKind(kind.into()),
        year: Some(2025),
        state,
        inked,
        is_folder,
    }
}

fn tok(kind: TokenKind, index: u32) -> Token {
    Token { kind, index }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Lowercase slug for the codename of a written file.
fn slug(name: &str) -> String {
    let stem = name.rsplit_once('.').map_or(name, |(s, _)| s);
    let mut out = String::new();
    for c in stem.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    out.trim_end_matches('-').to_string()
}

impl FakeBackend {
    /// The sample data set: a ready W-2, a held 1099, one still inking, and a folder.
    pub fn sample() -> Self {
        let w2 = FakeDoc {
            entry: entry("w2-acme-2025", "w2", DocState::Ready, 4, false),
            pages: vec![W2_PAGE.to_string()],
            fields: vec![
                FormField {
                    name: "box1_wages".into(),
                    value: "58,250.00".into(),
                    page: Some(1),
                },
                FormField {
                    name: "employee_ssn".into(),
                    value: "[SSN_1]".into(),
                    page: Some(1),
                },
            ],
            roles: vec![
                (tok(TokenKind::Ssn, 1), "employee"),
                (tok(TokenKind::Person, 1), "employee"),
                (tok(TokenKind::Ein, 1), "employer"),
                (tok(TokenKind::Org, 1), "employer"),
            ],
            children: vec![],
        };
        let held = FakeDoc {
            entry: entry(
                "1099-int-bank-2025",
                "1099-int",
                DocState::NeedsAttention,
                2,
                false,
            ),
            pages: vec![],
            fields: vec![],
            roles: vec![],
            children: vec![],
        };
        let inking = FakeDoc {
            entry: entry("1098-mortgage-2025", "1098", DocState::Inking, 0, false),
            pages: vec![],
            fields: vec![],
            roles: vec![],
            children: vec![],
        };
        let folder = FakeDoc {
            entry: entry("taxes-2025", "folder", DocState::Ready, 6, true),
            pages: vec![],
            fields: vec![],
            roles: vec![],
            children: vec![
                "w2-acme-2025".into(),
                "1099-int-bank-2025".into(),
                "1098-mortgage-2025".into(),
            ],
        };
        let docs: Vec<FakeDoc> = vec![w2, held, inking, folder];
        let order = docs
            .iter()
            .map(|d| d.entry.handle.codename.to_string())
            .collect();
        let docs = docs
            .into_iter()
            .map(|d| (d.entry.handle.codename.to_string(), d))
            .collect();
        Self {
            docs: Mutex::new(docs),
            order: Mutex::new(order),
            reads: Mutex::new(Vec::new()),
            writes: Mutex::new(BTreeMap::new()),
        }
    }

    /// `(agent, handle)` pairs passed to `note_read`, oldest first.
    pub fn reads(&self) -> Vec<(String, Handle)> {
        lock(&self.reads).clone()
    }

    /// Rehydrated contents of files saved through `write`, by codename.
    pub fn written(&self, codename: &str) -> Option<String> {
        lock(&self.writes).get(codename).cloned()
    }

    fn get(&self, h: &Handle) -> inker_core::Result<FakeDoc> {
        lock(&self.docs)
            .get(h.codename.as_str())
            .cloned()
            .ok_or_else(|| inker_core::Error::UnknownHandle(h.to_string()))
    }

    fn check_servable(doc: &FakeDoc) -> inker_core::Result<()> {
        let name = doc.entry.handle.to_string();
        match doc.entry.state {
            DocState::Ready => Ok(()),
            DocState::NeedsAttention => Err(inker_core::Error::PendingReview(name)),
            _ => Err(inker_core::Error::NotReady(name)),
        }
    }
}

impl Default for FakeBackend {
    fn default() -> Self {
        Self::sample()
    }
}

impl Backend for FakeBackend {
    fn list(&self) -> inker_core::Result<Vec<ListEntry>> {
        let docs = lock(&self.docs);
        Ok(lock(&self.order)
            .iter()
            .filter_map(|c| docs.get(c))
            .map(|d| d.entry.clone())
            .collect())
    }

    fn resolve(&self, h: &Handle) -> inker_core::Result<Resolved> {
        let doc = self.get(h)?;
        if doc.entry.is_folder {
            let docs = lock(&self.docs);
            let children = doc
                .children
                .iter()
                .filter_map(|c| docs.get(c))
                .map(|d| d.entry.clone())
                .collect();
            return Ok(Resolved::Folder(ResolvedFolder {
                handle: h.clone(),
                children,
            }));
        }
        Self::check_servable(&doc)?;
        Ok(Resolved::Document(ResolvedDocument {
            handle: h.clone(),
            kind: doc.entry.kind.clone(),
            year: doc.entry.year,
            pages: doc
                .pages
                .iter()
                .enumerate()
                .map(|(i, text)| ResolvedPage {
                    page: i as u32 + 1,
                    text: text.clone(),
                })
                .collect(),
            fields: doc.fields.clone(),
        }))
    }

    fn describe(&self, h: &Handle) -> inker_core::Result<Description> {
        let doc = self.get(h)?;
        if !doc.entry.is_folder {
            Self::check_servable(&doc)?;
        }
        let mut treatments: Vec<KindTreatment> = Vec::new();
        for (t, _) in &doc.roles {
            match treatments.iter_mut().find(|k| k.kind == t.kind) {
                Some(k) => k.count += 1,
                None => treatments.push(KindTreatment {
                    kind: t.kind.clone(),
                    treatment: Treatment::Tokenize,
                    count: 1,
                }),
            }
        }
        if !doc.pages.is_empty() {
            treatments.push(KindTreatment {
                kind: TokenKind::Address,
                treatment: Treatment::Generalize,
                count: 1,
            });
        }
        Ok(Description {
            handle: h.clone(),
            kind: doc.entry.kind.clone(),
            year: doc.entry.year,
            state: doc.entry.state,
            roles: doc
                .roles
                .iter()
                .map(|(t, r)| TokenRole {
                    token: t.clone(),
                    role: (*r).to_string(),
                })
                .collect(),
            treatments,
        })
    }

    fn write(&self, h: &Handle, name: &str, content: &str) -> inker_core::Result<WriteReceipt> {
        let source = self.get(h)?;
        Self::check_servable(&source)?;
        let suffix = slug(name);
        let base = if suffix.is_empty() {
            format!("{}-output", h.codename)
        } else {
            format!("{}-{suffix}", h.codename)
        };
        let mut codename = base.clone();
        let mut n = 2;
        while lock(&self.docs).contains_key(&codename) {
            codename = format!("{base}-{n}");
            n += 1;
        }
        let new_handle = Handle::new(
            Codename::parse(&codename)
                .map_err(|_| inker_core::Error::InvalidHandle(codename.clone()))?,
        );
        let mut real = content.to_string();
        for (token, value) in VAULT {
            real = real.replace(token, value);
        }
        lock(&self.writes).insert(codename.clone(), real);
        let doc = FakeDoc {
            entry: ListEntry {
                handle: new_handle.clone(),
                kind: DocKind("output".into()),
                year: source.entry.year,
                state: DocState::Ready,
                inked: 0,
                is_folder: false,
            },
            pages: vec![content.to_string()],
            fields: vec![],
            roles: source.roles.clone(),
            children: vec![],
        };
        lock(&self.docs).insert(codename.clone(), doc);
        lock(&self.order).push(codename.clone());
        Ok(WriteReceipt {
            path: format!("/private/{name}").into(),
            handle: new_handle,
        })
    }

    fn note_read(&self, agent: &str, h: &Handle) {
        lock(&self.reads).push((agent.to_string(), h.clone()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use inker_core::Error;

    fn h(s: &str) -> Handle {
        Handle::parse(s).unwrap()
    }

    #[test]
    fn sample_lists_the_four_entries() {
        let fake = FakeBackend::sample();
        let list = fake.list().unwrap();
        let names: Vec<String> = list.iter().map(|e| e.handle.to_string()).collect();
        assert_eq!(
            names,
            [
                "🐙/w2-acme-2025",
                "🐙/1099-int-bank-2025",
                "🐙/1098-mortgage-2025",
                "🐙/taxes-2025"
            ]
        );
        assert!(list[3].is_folder);
    }

    #[test]
    fn ready_document_resolves_tokenized() {
        let fake = FakeBackend::sample();
        let Resolved::Document(doc) = fake.resolve(&h("🐙/w2-acme-2025")).unwrap() else {
            panic!("expected a document");
        };
        let text = &doc.pages[0].text;
        assert!(text.contains("[SSN_1]") && text.contains("[EIN_1]"));
        for real in FAKE_REAL_VALUES {
            assert!(!text.contains(real));
        }
    }

    #[test]
    fn held_and_inking_documents_error() {
        let fake = FakeBackend::sample();
        assert!(matches!(
            fake.resolve(&h("🐙/1099-int-bank-2025")),
            Err(Error::PendingReview(_))
        ));
        assert!(matches!(
            fake.resolve(&h("🐙/1098-mortgage-2025")),
            Err(Error::NotReady(_))
        ));
        assert!(matches!(
            fake.resolve(&h("🐙/nope")),
            Err(Error::UnknownHandle(_))
        ));
    }

    #[test]
    fn folder_resolves_to_listing() {
        let fake = FakeBackend::sample();
        let Resolved::Folder(f) = fake.resolve(&h("🐙/taxes-2025")).unwrap() else {
            panic!("expected a folder");
        };
        assert_eq!(f.children.len(), 3);
    }

    #[test]
    fn describe_gives_roles() {
        let fake = FakeBackend::sample();
        let d = fake.describe(&h("🐙/w2-acme-2025")).unwrap();
        let roles: Vec<String> = d
            .roles
            .iter()
            .map(|r| format!("{} {}", r.token, r.role))
            .collect();
        assert!(roles.contains(&"[SSN_1] employee".to_string()));
        assert!(roles.contains(&"[EIN_1] employer".to_string()));
    }

    #[test]
    fn write_rehydrates_privately_and_mints_a_handle() {
        let fake = FakeBackend::sample();
        let r = fake
            .write(
                &h("🐙/w2-acme-2025"),
                "summary.md",
                "Wages for [PERSON_1] ([SSN_1])",
            )
            .unwrap();
        assert_eq!(r.handle.to_string(), "🐙/w2-acme-2025-summary");
        let saved = fake.written("w2-acme-2025-summary").unwrap();
        assert!(!saved.contains("[SSN_1]"));
        assert!(saved.contains(FAKE_REAL_VALUES[0]) || saved.contains(FAKE_REAL_VALUES[1]));
        // The new handle resolves back to the tokenized view.
        let Resolved::Document(doc) = fake.resolve(&r.handle).unwrap() else {
            panic!()
        };
        assert!(doc.pages[0].text.contains("[SSN_1]"));
    }

    #[test]
    fn note_read_is_recorded() {
        let fake = FakeBackend::sample();
        fake.note_read("claude-code", &h("🐙/w2-acme-2025"));
        assert_eq!(
            fake.reads(),
            vec![("claude-code".to_string(), h("🐙/w2-acme-2025"))]
        );
    }
}

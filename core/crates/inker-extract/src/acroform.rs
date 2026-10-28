//! AcroForm field names and values via lopdf.
//!
//! Walks the catalog's `/AcroForm /Fields` tree. A field's fully qualified
//! name joins the partial names (`/T`) of its ancestors with `.`. `/FT` and
//! `/V` are inheritable. A kid without `/T` is a widget of its parent, not a
//! field of its own. Only text (`/Tx`) and choice (`/Ch`) fields are returned:
//! buttons and signatures carry no extractable text.

use std::collections::{HashMap, HashSet};

use inker_core::{FormField, Result};
use lopdf::{Dictionary, Document, Object, ObjectId};

use crate::pdfdoc::decode_pdf_string;

/// Guard against hostile nesting.
const MAX_DEPTH: usize = 64;

pub(crate) fn read_fields(doc: &Document) -> Result<Vec<FormField>> {
    let Some(fields) = doc
        .catalog()
        .ok()
        .and_then(|c| deref_dict(doc, c.get(b"AcroForm").ok()?))
        .and_then(|af| deref_array(doc, af.get(b"Fields").ok()?))
    else {
        return Ok(Vec::new());
    };

    let pages = PageLookup::new(doc);
    let mut walker = Walker {
        doc,
        pages: &pages,
        seen: HashSet::new(),
        out: Vec::new(),
    };
    for f in fields {
        walker.walk(f, "", Inherited::default(), 0);
    }
    Ok(walker.out)
}

#[derive(Clone, Copy, Default)]
struct Inherited<'a> {
    ft: Option<&'a [u8]>,
    v: Option<&'a Object>,
}

struct Walker<'a> {
    doc: &'a Document,
    pages: &'a PageLookup,
    seen: HashSet<ObjectId>,
    out: Vec<FormField>,
}

impl<'a> Walker<'a> {
    fn walk(&mut self, obj: &'a Object, prefix: &str, inherited: Inherited<'a>, depth: usize) {
        if depth > MAX_DEPTH {
            return;
        }
        let Ok((id, obj)) = self.doc.dereference(obj) else {
            return;
        };
        if let Some(id) = id
            && !self.seen.insert(id)
        {
            return;
        }
        let Ok(dict) = obj.as_dict() else { return };

        let name = match dict.get(b"T").ok().and_then(|t| t.as_str().ok()) {
            Some(partial) if prefix.is_empty() => decode_pdf_string(partial),
            Some(partial) => format!("{prefix}.{}", decode_pdf_string(partial)),
            None => prefix.to_string(),
        };
        let here = Inherited {
            ft: dict
                .get(b"FT")
                .ok()
                .and_then(|o| o.as_name().ok())
                .or(inherited.ft),
            v: dict.get(b"V").ok().or(inherited.v),
        };

        let kids = dict
            .get(b"Kids")
            .ok()
            .and_then(|k| deref_array(self.doc, k))
            .unwrap_or(&[]);
        let field_kids: Vec<&'a Object> = kids.iter().filter(|k| self.has_name(k)).collect();
        if !field_kids.is_empty() {
            for kid in field_kids {
                self.walk(kid, &name, here, depth + 1);
            }
            return;
        }

        if name.is_empty() || !matches!(here.ft, Some(b"Tx" | b"Ch")) {
            return;
        }
        let page = self.page_of(id, dict).or_else(|| {
            kids.iter().find_map(|w| {
                self.doc
                    .dereference(w)
                    .ok()
                    .and_then(|(wid, wo)| self.page_of(wid, wo.as_dict().ok()?))
            })
        });
        let value = here
            .v
            .map(|v| value_string(self.doc, v, 0))
            .unwrap_or_default();
        self.out.push(FormField { name, value, page });
    }

    fn has_name(&self, kid: &Object) -> bool {
        self.doc
            .dereference(kid)
            .ok()
            .and_then(|(_, o)| o.as_dict().ok())
            .is_some_and(|d| d.has(b"T"))
    }

    fn page_of(&self, id: Option<ObjectId>, dict: &Dictionary) -> Option<u32> {
        dict.get(b"P")
            .ok()
            .and_then(|p| p.as_reference().ok())
            .and_then(|p| self.pages.by_page.get(&p).copied())
            .or_else(|| id.and_then(|id| self.pages.by_annot.get(&id).copied()))
    }
}

/// Page numbers by page object and by annotation object.
struct PageLookup {
    by_page: HashMap<ObjectId, u32>,
    by_annot: HashMap<ObjectId, u32>,
}

impl PageLookup {
    fn new(doc: &Document) -> Self {
        let mut by_page = HashMap::new();
        let mut by_annot = HashMap::new();
        for (num, page_id) in doc.get_pages() {
            by_page.insert(page_id, num);
            let annots = doc
                .get_dictionary(page_id)
                .ok()
                .and_then(|p| deref_array(doc, p.get(b"Annots").ok()?))
                .unwrap_or(&[]);
            for a in annots {
                if let Ok(a) = a.as_reference() {
                    by_annot.entry(a).or_insert(num);
                }
            }
        }
        Self { by_page, by_annot }
    }
}

fn deref_dict<'a>(doc: &'a Document, obj: &'a Object) -> Option<&'a Dictionary> {
    doc.dereference(obj).ok()?.1.as_dict().ok()
}

fn deref_array<'a>(doc: &'a Document, obj: &'a Object) -> Option<&'a [Object]> {
    doc.dereference(obj)
        .ok()?
        .1
        .as_array()
        .ok()
        .map(Vec::as_slice)
}

/// A field value as text: strings decoded, names as-is, arrays (multi-select
/// choice fields) joined with ", ".
fn value_string(doc: &Document, v: &Object, depth: usize) -> String {
    let Ok((_, v)) = doc.dereference(v) else {
        return String::new();
    };
    match v {
        Object::String(bytes, _) => decode_pdf_string(bytes),
        Object::Name(n) => String::from_utf8_lossy(n).into_owned(),
        Object::Array(items) if depth == 0 => items
            .iter()
            .map(|i| value_string(doc, i, depth + 1))
            .collect::<Vec<_>>()
            .join(", "),
        _ => String::new(),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use lopdf::{Dictionary, Document, Object, ObjectId, dictionary};

    use super::read_fields;

    /// A one-page document. `fields` builds the field tree given the page id
    /// and returns the top-level field ids.
    fn doc_with(fields: impl FnOnce(&mut Document, ObjectId) -> Vec<ObjectId>) -> Document {
        let mut doc = Document::with_version("1.7");
        let pages_id = doc.new_object_id();
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        });
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![page_id.into()],
                "Count" => 1,
            }),
        );
        let top = fields(&mut doc, page_id);
        let acroform = doc.add_object(dictionary! {
            "Fields" => top.into_iter().map(Object::Reference).collect::<Vec<_>>(),
        });
        let catalog = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
            "AcroForm" => acroform,
        });
        doc.trailer.set("Root", catalog);
        doc
    }

    fn widget(page: ObjectId, extra: Dictionary) -> Dictionary {
        let mut d = dictionary! { "Type" => "Annot", "Subtype" => "Widget", "P" => page };
        for (k, v) in extra.into_iter() {
            d.set(k, v);
        }
        d
    }

    fn names_values(doc: &Document) -> Vec<(String, String, Option<u32>)> {
        read_fields(doc)
            .unwrap()
            .into_iter()
            .map(|f| (f.name, f.value, f.page))
            .collect()
    }

    #[test]
    fn no_acroform_is_empty() {
        let mut doc = Document::with_version("1.7");
        let catalog = doc.add_object(dictionary! { "Type" => "Catalog" });
        doc.trailer.set("Root", catalog);
        assert!(read_fields(&doc).unwrap().is_empty());
    }

    #[test]
    fn flat_text_field() {
        let doc = doc_with(|doc, page| {
            vec![doc.add_object(widget(
                page,
                dictionary! { "FT" => "Tx", "T" => Object::string_literal("box1"), "V" => Object::string_literal("1.00") },
            ))]
        });
        assert_eq!(
            names_values(&doc),
            vec![("box1".into(), "1.00".into(), Some(1))]
        );
    }

    #[test]
    fn kids_build_qualified_names_and_inherit_type() {
        let doc = doc_with(|doc, page| {
            let parent = doc.new_object_id();
            let a = doc.add_object(widget(
                page,
                dictionary! { "Parent" => parent, "T" => Object::string_literal("a"), "V" => Object::string_literal("x") },
            ));
            let b = doc.add_object(widget(
                page,
                dictionary! { "Parent" => parent, "T" => Object::string_literal("b"), "V" => Object::string_literal("y") },
            ));
            doc.objects.insert(
                parent,
                Object::Dictionary(dictionary! {
                    "FT" => "Tx",
                    "T" => Object::string_literal("emp"),
                    "Kids" => vec![a.into(), b.into()],
                }),
            );
            vec![parent]
        });
        assert_eq!(
            names_values(&doc),
            vec![
                ("emp.a".into(), "x".into(), Some(1)),
                ("emp.b".into(), "y".into(), Some(1))
            ]
        );
    }

    #[test]
    fn widget_kids_without_names_belong_to_their_field() {
        // One field shown in two places: the value lives on the parent.
        let doc = doc_with(|doc, page| {
            let parent = doc.new_object_id();
            let w1 = doc.add_object(widget(page, dictionary! { "Parent" => parent }));
            let w2 = doc.add_object(widget(page, dictionary! { "Parent" => parent }));
            doc.objects.insert(
                parent,
                Object::Dictionary(dictionary! {
                    "FT" => "Tx",
                    "T" => Object::string_literal("ssn"),
                    "V" => Object::string_literal("000-12-3456"),
                    "Kids" => vec![w1.into(), w2.into()],
                }),
            );
            vec![parent]
        });
        assert_eq!(
            names_values(&doc),
            vec![("ssn".into(), "000-12-3456".into(), Some(1))]
        );
    }

    #[test]
    fn choice_values_name_and_array() {
        let doc = doc_with(|doc, page| {
            vec![
                doc.add_object(widget(
                    page,
                    dictionary! { "FT" => "Ch", "T" => Object::string_literal("state"), "V" => Object::string_literal("CO") },
                )),
                doc.add_object(widget(
                    page,
                    dictionary! {
                        "FT" => "Ch",
                        "T" => Object::string_literal("codes"),
                        "V" => vec![Object::string_literal("D"), Object::string_literal("W")],
                    },
                )),
                doc.add_object(widget(
                    page,
                    dictionary! { "FT" => "Ch", "T" => Object::string_literal("named"), "V" => Object::Name(b"OR".to_vec()) },
                )),
            ]
        });
        assert_eq!(
            names_values(&doc),
            vec![
                ("state".into(), "CO".into(), Some(1)),
                ("codes".into(), "D, W".into(), Some(1)),
                ("named".into(), "OR".into(), Some(1)),
            ]
        );
    }

    #[test]
    fn utf16_name_and_value() {
        let doc = doc_with(|doc, page| {
            let utf16 = |s: &str| {
                let mut b = vec![0xFE, 0xFF];
                b.extend(s.encode_utf16().flat_map(|u| u.to_be_bytes()));
                Object::String(b, lopdf::StringFormat::Hexadecimal)
            };
            vec![doc.add_object(widget(
                page,
                dictionary! { "FT" => "Tx", "T" => utf16("nom"), "V" => utf16("Ñandú") },
            ))]
        });
        assert_eq!(
            names_values(&doc),
            vec![("nom".into(), "Ñandú".into(), Some(1))]
        );
    }

    #[test]
    fn fields_without_value_are_kept_empty_and_buttons_skipped() {
        let doc = doc_with(|doc, page| {
            vec![
                doc.add_object(widget(page, dictionary! { "FT" => "Tx", "T" => Object::string_literal("blank") })),
                doc.add_object(widget(
                    page,
                    dictionary! { "FT" => "Btn", "T" => Object::string_literal("box13"), "V" => Object::Name(b"Yes".to_vec()) },
                )),
                doc.add_object(widget(
                    page,
                    dictionary! { "FT" => "Sig", "T" => Object::string_literal("sig") },
                )),
            ]
        });
        assert_eq!(
            names_values(&doc),
            vec![("blank".into(), String::new(), Some(1))]
        );
    }

    #[test]
    fn page_found_through_annots_when_p_is_missing() {
        let doc = doc_with(|doc, page| {
            let f = doc.add_object(dictionary! {
                "Subtype" => "Widget", "FT" => "Tx", "T" => Object::string_literal("f"), "V" => Object::string_literal("v"),
            });
            doc.get_dictionary_mut(page)
                .unwrap()
                .set("Annots", vec![Object::Reference(f)]);
            vec![f]
        });
        assert_eq!(names_values(&doc), vec![("f".into(), "v".into(), Some(1))]);
    }

    #[test]
    fn cycles_in_kids_terminate() {
        let doc = doc_with(|doc, page| {
            let a = doc.new_object_id();
            doc.objects.insert(
                a,
                Object::Dictionary(dictionary! { "FT" => "Tx", "T" => Object::string_literal("loop"), "Kids" => vec![a.into()] }),
            );
            let _ = page;
            vec![a]
        });
        // Must return, whatever it finds.
        let _ = read_fields(&doc).unwrap();
    }
}

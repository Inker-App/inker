//! JSON that keeps key order, without turning on serde_json's
//! `preserve_order` for the whole workspace.

use indexmap::IndexMap;

pub type Obj = IndexMap<String, J>;

/// An order-preserving JSON value.
#[derive(Debug, Clone, PartialEq)]
pub enum J {
    Null,
    Bool(bool),
    Num(serde_json::Number),
    Str(String),
    Arr(Vec<J>),
    Obj(Obj),
}

pub fn parse(s: &str) -> Result<J, String> {
    serde_json::from_str(s).map_err(|e| e.to_string())
}

/// Two-space pretty JSON with a trailing newline.
pub fn to_string_pretty(v: &J) -> String {
    let mut s = serde_json::to_string_pretty(v).unwrap_or_else(|_| "null".into());
    s.push('\n');
    s
}

/// Walk (creating or replacing non-objects as needed) to the object at `path`.
pub fn obj_path<'a>(v: &'a mut J, path: &[&str]) -> &'a mut Obj {
    let mut cur = v;
    if !matches!(cur, J::Obj(_)) {
        *cur = J::Obj(Obj::new());
    }
    for key in path {
        let J::Obj(o) = cur else {
            unreachable!("ensured object above")
        };
        let next = o
            .entry((*key).to_string())
            .or_insert_with(|| J::Obj(Obj::new()));
        if !matches!(next, J::Obj(_)) {
            *next = J::Obj(Obj::new());
        }
        cur = next;
    }
    match cur {
        J::Obj(o) => o,
        _ => unreachable!("ensured object above"),
    }
}

/// Look up a nested value without creating anything.
pub fn get<'a>(v: &'a J, path: &[&str]) -> Option<&'a J> {
    let mut cur = v;
    for key in path {
        let J::Obj(o) = cur else { return None };
        cur = o.get(*key)?;
    }
    Some(cur)
}

/// Append `s` to an array unless present. Returns whether it was added.
/// A non-array is replaced by an array.
pub fn push_unique_str(arr: &mut J, s: &str) -> bool {
    if !matches!(arr, J::Arr(_)) {
        *arr = J::Arr(Vec::new());
    }
    let J::Arr(items) = arr else {
        unreachable!("ensured array above")
    };
    if items.iter().any(|i| matches!(i, J::Str(x) if x == s)) {
        return false;
    }
    items.push(J::Str(s.to_string()));
    true
}

impl J {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            J::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&Vec<J>> {
        match self {
            J::Arr(a) => Some(a),
            _ => None,
        }
    }

    pub fn str_array(items: &[&str]) -> J {
        J::Arr(items.iter().map(|s| J::Str((*s).to_string())).collect())
    }

    /// True if any string anywhere inside equals `needle`.
    pub fn contains_str(&self, needle: &str) -> bool {
        match self {
            J::Str(s) => s == needle,
            J::Arr(a) => a.iter().any(|v| v.contains_str(needle)),
            J::Obj(o) => o.values().any(|v| v.contains_str(needle)),
            _ => false,
        }
    }
}

impl serde::Serialize for J {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::{SerializeMap, SerializeSeq};
        match self {
            J::Null => s.serialize_unit(),
            J::Bool(b) => s.serialize_bool(*b),
            J::Num(n) => n.serialize(s),
            J::Str(x) => s.serialize_str(x),
            J::Arr(a) => {
                let mut seq = s.serialize_seq(Some(a.len()))?;
                for v in a {
                    seq.serialize_element(v)?;
                }
                seq.end()
            }
            J::Obj(o) => {
                let mut map = s.serialize_map(Some(o.len()))?;
                for (k, v) in o {
                    map.serialize_entry(k, v)?;
                }
                map.end()
            }
        }
    }
}

impl<'de> serde::Deserialize<'de> for J {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> serde::de::Visitor<'de> for V {
            type Value = J;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("any JSON value")
            }
            fn visit_unit<E>(self) -> Result<J, E> {
                Ok(J::Null)
            }
            fn visit_none<E>(self) -> Result<J, E> {
                Ok(J::Null)
            }
            fn visit_bool<E>(self, b: bool) -> Result<J, E> {
                Ok(J::Bool(b))
            }
            fn visit_i64<E>(self, n: i64) -> Result<J, E> {
                Ok(J::Num(n.into()))
            }
            fn visit_u64<E>(self, n: u64) -> Result<J, E> {
                Ok(J::Num(n.into()))
            }
            fn visit_f64<E: serde::de::Error>(self, n: f64) -> Result<J, E> {
                serde_json::Number::from_f64(n)
                    .map(J::Num)
                    .ok_or_else(|| E::custom("non-finite number"))
            }
            fn visit_str<E>(self, s: &str) -> Result<J, E> {
                Ok(J::Str(s.to_string()))
            }
            fn visit_string<E>(self, s: String) -> Result<J, E> {
                Ok(J::Str(s))
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut a: A) -> Result<J, A::Error> {
                let mut v = Vec::new();
                while let Some(x) = a.next_element()? {
                    v.push(x);
                }
                Ok(J::Arr(v))
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(self, mut a: A) -> Result<J, A::Error> {
                let mut o = Obj::new();
                while let Some((k, v)) = a.next_entry::<String, J>()? {
                    o.insert(k, v);
                }
                Ok(J::Obj(o))
            }
        }
        d.deserialize_any(V)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_keeps_order_and_values() {
        let src = r#"{"z": 1, "a": [true, null, 1.5, "s"], "m": {"y": {}, "b": []}}"#;
        let v = parse(src).unwrap();
        let out = to_string_pretty(&v);
        assert!(out.find("\"z\"").unwrap() < out.find("\"a\"").unwrap());
        assert!(out.find("\"y\"").unwrap() < out.find("\"b\"").unwrap());
        let a: serde_json::Value = serde_json::from_str(src).unwrap();
        let b: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(a, b);
        assert!(out.ends_with("}\n"));
    }

    #[test]
    fn rejects_non_json() {
        assert!(parse("{ nope").is_err());
    }

    #[test]
    fn object_helpers() {
        let mut v = parse("{}").unwrap();
        let arr = obj_path(&mut v, &["a", "b"])
            .entry("list".to_string())
            .or_insert_with(|| J::Arr(vec![]));
        assert!(push_unique_str(arr, "x"));
        assert!(!push_unique_str(arr, "x"));
        assert_eq!(
            to_string_pretty(&v),
            "{\n  \"a\": {\n    \"b\": {\n      \"list\": [\n        \"x\"\n      ]\n    }\n  }\n}\n"
        );
    }
}

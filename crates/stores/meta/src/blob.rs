//! The object format of the Meta Horizon Link app's local cache (`data.sqlite`, table `Objects`).
//!
//! Each row holds one GraphQL object, serialized as:
//!
//! - `u32` format version (4), then the object's type name as a string (`u32` length + UTF-8),
//! - its fields, each a tag byte, the field name (`u32` length + UTF-8) and the value, ended by a
//!   `0` byte instead of a tag.
//!
//! A value starts with a "present" byte (`0` = null). A reference (`r`) then names the type
//! (empty = null) and either a key (`1`, then a `u64`-length string: the object lives in its own
//! row) or `0` and the object's fields inline. Every other kind has a second "not null" byte and
//! then: `s` a `u64`-length string, `S` a `u64` count of such strings, `c` an `i64`, `i` an `i32`,
//! `d` an `f64`, `b` a byte, `v` a `u64` count of references (without the present byte).
//!
//! The layout was worked out from the files the app writes; unknown tags make the row unreadable
//! (it is skipped), so a format change degrades to fewer details rather than wrong ones.

/// A value of a cached object.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Str(String),
    Strs(Vec<String>),
    Int(i64),
    Float(f64),
    Bool(bool),
    /// A reference to an object stored in its own row.
    Ref {
        type_name: String,
        key: String,
    },
    Object(Object),
    List(Vec<Value>),
}

/// One cached object: its type and fields in stored order.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Object {
    pub type_name: String,
    pub fields: Vec<(String, Value)>,
}

impl Object {
    pub fn get(&self, name: &str) -> Option<&Value> {
        self.fields.iter().find(|(n, _)| n == name).map(|(_, v)| v)
    }

    /// A non-empty string field.
    pub fn str(&self, name: &str) -> Option<&str> {
        match self.get(name)? {
            Value::Str(s) if !s.trim().is_empty() => Some(s.as_str()),
            _ => None,
        }
    }

    pub fn object(&self, name: &str) -> Option<&Object> {
        match self.get(name)? {
            Value::Object(o) => Some(o),
            _ => None,
        }
    }

    pub fn bool(&self, name: &str) -> Option<bool> {
        match self.get(name)? {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }

    /// An integer field (`c` or `i`).
    pub fn int(&self, name: &str) -> Option<i64> {
        match self.get(name)? {
            Value::Int(i) => Some(*i),
            _ => None,
        }
    }

    pub fn float(&self, name: &str) -> Option<f64> {
        match self.get(name)? {
            Value::Float(f) => Some(*f),
            Value::Int(i) => Some(*i as f64),
            _ => None,
        }
    }

    pub fn strs(&self, name: &str) -> Vec<String> {
        match self.get(name) {
            Some(Value::Strs(s)) => s.clone(),
            _ => Vec::new(),
        }
    }

    /// The key of a reference field.
    pub fn ref_key(&self, name: &str) -> Option<&str> {
        match self.get(name)? {
            Value::Ref { key, .. } => Some(key.as_str()),
            _ => None,
        }
    }

    /// The type of a reference or inline object field.
    pub fn type_of(&self, name: &str) -> Option<&str> {
        match self.get(name)? {
            Value::Ref { type_name, .. } => Some(type_name.as_str()),
            Value::Object(o) => Some(o.type_name.as_str()),
            _ => None,
        }
    }
}

struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
    depth: usize,
}

/// Nested objects deeper than this are treated as damage.
const MAX_DEPTH: usize = 64;

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        let end = self
            .pos
            .checked_add(n)
            .filter(|&end| end <= self.buf.len())
            .ok_or_else(|| format!("truncated at byte {}", self.pos))?;
        let bytes = &self.buf[self.pos..end];
        self.pos = end;
        Ok(bytes)
    }

    fn u8(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N], String> {
        let mut out = [0; N];
        out.copy_from_slice(self.take(N)?);
        Ok(out)
    }

    fn u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_le_bytes(self.array()?))
    }

    fn u64(&mut self) -> Result<u64, String> {
        Ok(u64::from_le_bytes(self.array()?))
    }

    /// A length read from the data, checked against what is left so damage can't allocate much.
    fn len(&mut self, len: u64) -> Result<usize, String> {
        usize::try_from(len)
            .ok()
            .filter(|&n| n <= self.buf.len() - self.pos)
            .ok_or_else(|| format!("bad length {len} at byte {}", self.pos))
    }

    fn text(&mut self, len: usize) -> Result<String, String> {
        Ok(String::from_utf8_lossy(self.take(len)?).into_owned())
    }

    fn str32(&mut self) -> Result<String, String> {
        let n = self.u32()?;
        let n = self.len(u64::from(n))?;
        self.text(n)
    }

    fn str64(&mut self) -> Result<String, String> {
        let n = self.u64()?;
        let n = self.len(n)?;
        self.text(n)
    }

    fn fields(&mut self, type_name: String) -> Result<Object, String> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return Err("objects nested too deeply".into());
        }
        let mut object = Object {
            type_name,
            fields: Vec::new(),
        };
        loop {
            let tag = self.u8()?;
            if tag == 0 {
                break;
            }
            let name = self.str32()?;
            let value = self.value(tag)?;
            object.fields.push((name, value));
        }
        self.depth -= 1;
        Ok(object)
    }

    /// A reference without its present byte (as in lists).
    fn reference(&mut self) -> Result<Value, String> {
        let type_name = self.str32()?;
        if type_name.is_empty() {
            return Ok(Value::Null);
        }
        match self.u8()? {
            1 => Ok(Value::Ref {
                type_name,
                key: self.str64()?,
            }),
            0 => Ok(Value::Object(self.fields(type_name)?)),
            other => Err(format!(
                "unknown reference kind {other} at byte {}",
                self.pos
            )),
        }
    }

    fn value(&mut self, tag: u8) -> Result<Value, String> {
        if self.u8()? == 0 {
            return Ok(Value::Null);
        }
        if tag == b'r' {
            return self.reference();
        }
        if self.u8()? == 0 {
            return Ok(Value::Null);
        }
        Ok(match tag {
            b's' => Value::Str(self.str64()?),
            b'S' => {
                let count = self.u64()?;
                let count = self.len(count)?;
                let mut items = Vec::with_capacity(count.min(1024));
                for _ in 0..count {
                    items.push(self.str64()?);
                }
                Value::Strs(items)
            }
            b'c' => Value::Int(i64::from_le_bytes(self.array()?)),
            b'i' => Value::Int(i64::from(i32::from_le_bytes(self.array()?))),
            b'd' => Value::Float(f64::from_le_bytes(self.array()?)),
            b'b' => Value::Bool(self.u8()? != 0),
            b'v' => {
                let count = self.u64()?;
                let count = self.len(count)?;
                let mut items = Vec::with_capacity(count.min(1024));
                for _ in 0..count {
                    items.push(self.reference()?);
                }
                Value::List(items)
            }
            other => {
                return Err(format!(
                    "unknown field kind {:?} at byte {}",
                    char::from(other),
                    self.pos
                ));
            }
        })
    }
}

/// Parses one `Objects.value` blob. The error says where it stopped making sense.
pub fn parse(blob: &[u8]) -> Result<Object, String> {
    let mut reader = Reader {
        buf: blob,
        pos: 0,
        depth: 0,
    };
    let _version = reader.u32()?;
    let type_name = reader.str32()?;
    let object = reader.fields(type_name)?;
    if reader.pos != blob.len() {
        return Err(format!(
            "{} bytes left after the object",
            blob.len() - reader.pos
        ));
    }
    Ok(object)
}

/// Writes objects in the cache format, for tests and fixtures.
#[cfg(test)]
pub mod encode {
    use super::{Object, Value};

    fn str32(out: &mut Vec<u8>, s: &str) {
        out.extend_from_slice(&(s.len() as u32).to_le_bytes());
        out.extend_from_slice(s.as_bytes());
    }

    fn str64(out: &mut Vec<u8>, s: &str) {
        out.extend_from_slice(&(s.len() as u64).to_le_bytes());
        out.extend_from_slice(s.as_bytes());
    }

    fn reference(out: &mut Vec<u8>, value: &Value) {
        match value {
            Value::Ref { type_name, key } => {
                str32(out, type_name);
                out.push(1);
                str64(out, key);
            }
            Value::Object(o) => {
                str32(out, &o.type_name);
                out.push(0);
                fields(out, o);
            }
            _ => str32(out, ""),
        }
    }

    fn fields(out: &mut Vec<u8>, object: &Object) {
        for (name, value) in &object.fields {
            let tag = match value {
                Value::Null | Value::Str(_) => b's',
                Value::Strs(_) => b'S',
                Value::Int(_) => b'c',
                Value::Float(_) => b'd',
                Value::Bool(_) => b'b',
                Value::Ref { .. } | Value::Object(_) => b'r',
                Value::List(_) => b'v',
            };
            out.push(tag);
            str32(out, name);
            match value {
                Value::Null => out.push(0),
                Value::Ref { .. } | Value::Object(_) => {
                    out.push(1);
                    reference(out, value);
                }
                _ => {
                    out.extend_from_slice(&[1, 1]);
                    match value {
                        Value::Str(s) => str64(out, s),
                        Value::Strs(items) => {
                            out.extend_from_slice(&(items.len() as u64).to_le_bytes());
                            for s in items {
                                str64(out, s);
                            }
                        }
                        Value::Int(i) => out.extend_from_slice(&i.to_le_bytes()),
                        Value::Float(f) => out.extend_from_slice(&f.to_le_bytes()),
                        Value::Bool(b) => out.push(u8::from(*b)),
                        Value::List(items) => {
                            out.extend_from_slice(&(items.len() as u64).to_le_bytes());
                            for item in items {
                                reference(out, item);
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        out.push(0);
    }

    pub fn encode(object: &Object) -> Vec<u8> {
        let mut out = 4u32.to_le_bytes().to_vec();
        str32(&mut out, &object.type_name);
        fields(&mut out, object);
        out
    }

    /// An object from `(name, value)` pairs.
    pub fn object(type_name: &str, fields: Vec<(&str, Value)>) -> Object {
        Object {
            type_name: type_name.into(),
            fields: fields.into_iter().map(|(n, v)| (n.into(), v)).collect(),
        }
    }

    pub fn s(text: &str) -> Value {
        Value::Str(text.into())
    }

    pub fn r(type_name: &str, key: &str) -> Value {
        Value::Ref {
            type_name: type_name.into(),
            key: key.into(),
        }
    }

    pub fn image(uri: &str) -> Value {
        Value::Object(object("Image", vec![("uri", s(uri))]))
    }
}

#[cfg(test)]
mod tests {
    use super::encode::*;
    use super::*;

    /// An `Entitlement` row as the app writes it (ids and token made up), byte for byte.
    const ENTITLEMENT: &[u8] = include_bytes!("../fixtures/entitlement.bin");

    #[test]
    fn parses_a_stored_entitlement() {
        let o = parse(ENTITLEMENT).unwrap();
        assert_eq!(o.type_name, "Entitlement");
        assert_eq!(o.str("active_state"), Some("PERMANENT"));
        assert_eq!(o.str("id"), Some("1000000000001:2000000000000002"));
        assert_eq!(o.bool("is_active"), Some(true));
        assert_eq!(o.bool("is_hidden"), Some(false));
        assert_eq!(o.int("expiration_time"), Some(0));
        assert_eq!(o.type_of("item"), Some("Application"));
        assert_eq!(o.ref_key("item"), Some("2000000000000002"));
        assert_eq!(o.str("signed_token"), Some("made-up-token"));
    }

    #[test]
    fn round_trips_every_kind_of_value() {
        let inner = object(
            "AppStoreOffer",
            vec![(
                "price",
                Value::Object(object("CurrencyQuantity", vec![("formatted", s("1,99"))])),
            )],
        );
        let o = object(
            "Application",
            vec![
                ("display_name", s("Moon Garden")),
                ("missing", Value::Null),
                ("keys", Value::Strs(vec!["IAP".into(), "FRIENDS".into()])),
                ("time", Value::Int(-5)),
                ("rating", Value::Float(4.5)),
                ("flag", Value::Bool(true)),
                ("grouping", r("ApplicationGrouping", "77")),
                ("offer", Value::Object(inner)),
                ("cover_square_image", image("https://example.invalid/a.jpg")),
                (
                    "edges",
                    Value::List(vec![
                        r("Entitlement", "1:2"),
                        Value::Object(object("Edge", vec![])),
                    ]),
                ),
            ],
        );
        assert_eq!(parse(&encode(&o)).unwrap(), o);
        assert_eq!(
            o.object("cover_square_image").and_then(|i| i.str("uri")),
            Some("https://example.invalid/a.jpg")
        );
    }

    #[test]
    fn empty_reference_types_are_null() {
        // `r language` with an empty type name: no reference kind follows.
        let mut blob = 4u32.to_le_bytes().to_vec();
        blob.extend_from_slice(&3u32.to_le_bytes());
        blob.extend_from_slice(b"App");
        blob.push(b'r');
        blob.extend_from_slice(&8u32.to_le_bytes());
        blob.extend_from_slice(b"language");
        blob.push(1);
        blob.extend_from_slice(&0u32.to_le_bytes());
        blob.push(0);
        let o = parse(&blob).unwrap();
        assert_eq!(o.get("language"), Some(&Value::Null));
    }

    #[test]
    fn rejects_damaged_blobs() {
        let blob = encode(&object(
            "Application",
            vec![("display_name", s("Moon Garden"))],
        ));
        assert!(parse(&blob[..blob.len() - 3]).is_err());
        let mut longer = blob.clone();
        longer.push(7);
        assert!(parse(&longer).is_err());
        let mut unknown = blob;
        let tag = unknown.iter().position(|&b| b == b's').unwrap();
        unknown[tag] = b'?';
        assert!(parse(&unknown).unwrap_err().contains("unknown field kind"));
        // A huge length must not allocate.
        let mut huge = 4u32.to_le_bytes().to_vec();
        huge.extend_from_slice(&u32::MAX.to_le_bytes());
        assert!(parse(&huge).is_err());
    }
}

//! JSON helpers for the manifest: exact numbers and a stable, diff-friendly layout.

use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

/// A number in the manifest.
///
/// Finite values are JSON numbers written with the fewest digits that read back to
/// the same bits. JSON has no spelling for the other values, so they are written as
/// the strings `"NaN"`, `"Infinity"` and `"-Infinity"`; a document holding one can
/// still be saved.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub(crate) struct Num(pub f64);

impl Num {
    /// The number to write for an `f32`: the shortest decimal that reads back to it.
    ///
    /// JSON readers parse to `f64` and then narrow, and for a handful of values that
    /// double rounding does not land on the original. Those are written with the
    /// digits of the widened value instead, which always narrows back exactly.
    pub fn from_f32(x: f32) -> Self {
        if !x.is_finite() {
            return Num(x as f64);
        }
        let short: f64 = x.to_string().parse().unwrap_or(x as f64);
        if (short as f32).to_bits() == x.to_bits() {
            Num(short)
        } else {
            Num(x as f64)
        }
    }

    pub fn to_f32(self) -> f32 {
        self.0 as f32
    }
}

impl From<f64> for Num {
    fn from(x: f64) -> Self {
        Num(x)
    }
}

impl From<f32> for Num {
    fn from(x: f32) -> Self {
        Num::from_f32(x)
    }
}

impl Serialize for Num {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if self.0.is_finite() {
            serializer.serialize_f64(self.0)
        } else if self.0.is_nan() {
            serializer.serialize_str("NaN")
        } else if self.0 > 0.0 {
            serializer.serialize_str("Infinity")
        } else {
            serializer.serialize_str("-Infinity")
        }
    }
}

impl<'de> Deserialize<'de> for Num {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct NumVisitor;
        impl Visitor<'_> for NumVisitor {
            type Value = Num;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a number, or \"NaN\", \"Infinity\" or \"-Infinity\"")
            }

            fn visit_f64<E: de::Error>(self, v: f64) -> Result<Num, E> {
                Ok(Num(v))
            }

            fn visit_i64<E: de::Error>(self, v: i64) -> Result<Num, E> {
                Ok(Num(v as f64))
            }

            fn visit_u64<E: de::Error>(self, v: u64) -> Result<Num, E> {
                Ok(Num(v as f64))
            }

            fn visit_str<E: de::Error>(self, v: &str) -> Result<Num, E> {
                match v {
                    "NaN" => Ok(Num(f64::NAN)),
                    "Infinity" => Ok(Num(f64::INFINITY)),
                    "-Infinity" => Ok(Num(f64::NEG_INFINITY)),
                    other => Err(E::invalid_value(de::Unexpected::Str(other), &self)),
                }
            }
        }
        deserializer.deserialize_any(NumVisitor)
    }
}

pub(crate) fn nums<const N: usize>(values: [f32; N]) -> [Num; N] {
    values.map(Num::from_f32)
}

pub(crate) fn f32s<const N: usize>(values: [Num; N]) -> [f32; N] {
    values.map(Num::to_f32)
}

// ----- layout -----

enum Node<'a> {
    Scalar(&'a str),
    Array(Vec<Node<'a>>),
    Object(Vec<(&'a str, Node<'a>)>),
}

impl Node<'_> {
    /// A scalar or an empty container: something that never needs a line of its own.
    fn is_leaf(&self) -> bool {
        match self {
            Node::Scalar(_) => true,
            Node::Array(items) => items.is_empty(),
            Node::Object(members) => members.is_empty(),
        }
    }

    /// The layout rule, which depends only on the shape of the data:
    /// an array is written on one line when it holds only leaves; an object is
    /// written on one line when it sits at least two levels below the top and
    /// every member is a leaf or a one-line array.
    fn is_inline(&self, depth: usize) -> bool {
        match self {
            Node::Scalar(_) => true,
            Node::Array(items) => items.iter().all(Node::is_leaf),
            Node::Object(members) => {
                members.is_empty()
                    || (depth >= 2
                        && members.iter().all(|(_, value)| match value {
                            Node::Array(items) => items.iter().all(Node::is_leaf),
                            other => other.is_leaf(),
                        }))
            }
        }
    }
}

struct Parser<'a> {
    text: &'a str,
    at: usize,
}

impl<'a> Parser<'a> {
    fn peek(&self) -> u8 {
        self.text.as_bytes().get(self.at).copied().unwrap_or(0)
    }

    fn skip_space(&mut self) {
        while matches!(self.peek(), b' ' | b'\n' | b'\r' | b'\t') {
            self.at += 1;
        }
    }

    fn string(&mut self) -> &'a str {
        let start = self.at;
        self.at += 1;
        loop {
            match self.peek() {
                b'\\' => self.at += 2,
                b'"' | 0 => {
                    self.at += 1;
                    break;
                }
                _ => self.at += 1,
            }
        }
        let end = self.at.min(self.text.len());
        &self.text[start..end]
    }

    fn node(&mut self) -> Node<'a> {
        self.skip_space();
        match self.peek() {
            b'[' => {
                self.at += 1;
                let mut items = Vec::new();
                loop {
                    self.skip_space();
                    match self.peek() {
                        b']' | 0 => {
                            self.at += 1;
                            break;
                        }
                        b',' => self.at += 1,
                        _ => items.push(self.node()),
                    }
                }
                Node::Array(items)
            }
            b'{' => {
                self.at += 1;
                let mut members = Vec::new();
                loop {
                    self.skip_space();
                    match self.peek() {
                        b'}' | 0 => {
                            self.at += 1;
                            break;
                        }
                        b',' => self.at += 1,
                        _ => {
                            let key = self.string();
                            self.skip_space();
                            if self.peek() == b':' {
                                self.at += 1;
                            }
                            members.push((key, self.node()));
                        }
                    }
                }
                Node::Object(members)
            }
            b'"' => Node::Scalar(self.string()),
            _ => {
                let start = self.at;
                while !matches!(
                    self.peek(),
                    b',' | b']' | b'}' | b' ' | b'\n' | b'\r' | b'\t' | 0
                ) {
                    self.at += 1;
                }
                Node::Scalar(&self.text[start..self.at])
            }
        }
    }
}

fn indent(out: &mut String, depth: usize) {
    out.push('\n');
    for _ in 0..depth {
        out.push_str("  ");
    }
}

fn write_node(out: &mut String, node: &Node, depth: usize) {
    let inline = node.is_inline(depth);
    match node {
        Node::Scalar(text) => out.push_str(text),
        Node::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                if inline {
                    if i > 0 {
                        out.push(' ');
                    }
                } else {
                    indent(out, depth + 1);
                }
                write_node(out, item, depth + 1);
            }
            if !inline {
                indent(out, depth);
            }
            out.push(']');
        }
        Node::Object(members) => {
            out.push('{');
            for (i, (key, value)) in members.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                if inline {
                    if i > 0 {
                        out.push(' ');
                    }
                } else {
                    indent(out, depth + 1);
                }
                out.push_str(key);
                out.push_str(": ");
                write_node(out, value, depth + 1);
            }
            if !inline {
                indent(out, depth);
            }
            out.push('}');
        }
    }
}

/// Serialises `value` in the manifest's layout: two-space indentation, `\n` line
/// ends, members in declaration order, and short records (keys, transforms, table
/// rows, arrays of numbers) on one line each so a change shows up as a one-line diff.
pub(crate) fn to_pretty_string<T: Serialize>(value: &T) -> Result<String, serde_json::Error> {
    let compact = serde_json::to_string(value)?;
    let node = Parser {
        text: &compact,
        at: 0,
    }
    .node();
    let mut out = String::with_capacity(compact.len() * 2);
    write_node(&mut out, &node, 0);
    out.push('\n');
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn layout_is_stable_and_parses_back() {
        let value = json!({
            "a": 1,
            "settings": {"title": "x \"quoted\" {[,]} \\", "list": [1, 2.5, -3e-7], "empty": []},
            "table": [{"id": 1, "blob": "aa"}, {"id": 2, "blob": "bb"}],
            "tree": {"name": "n", "transform": {"rotation": [0.0, 0.0, 0.0, 1.0], "scale": 1.0},
                     "children": [{"name": "c", "transform": {"scale": 2.0}, "keys": [{"time": 0, "value": true}]}]},
            "palette": [[1, 2, 3], [4, 5, 6]]
        });
        let text = to_pretty_string(&value).unwrap();
        let expected = r#"{
  "a": 1,
  "palette": [
    [1, 2, 3],
    [4, 5, 6]
  ],
  "settings": {
    "empty": [],
    "list": [1, 2.5, -3e-7],
    "title": "x \"quoted\" {[,]} \\"
  },
  "table": [
    {"blob": "aa", "id": 1},
    {"blob": "bb", "id": 2}
  ],
  "tree": {
    "children": [
      {
        "keys": [
          {"time": 0, "value": true}
        ],
        "name": "c",
        "transform": {"scale": 2.0}
      }
    ],
    "name": "n",
    "transform": {"rotation": [0.0, 0.0, 0.0, 1.0], "scale": 1.0}
  }
}
"#;
        assert_eq!(text, expected);
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&text).unwrap(),
            value
        );
    }

    #[test]
    fn every_f32_bit_pattern_tried_survives_json() {
        let mut state = 0x9E37_79B9_7F4A_7C15u64;
        let mut values = vec![
            0.0f32,
            -0.0,
            1.0,
            0.1,
            0.8,
            f32::MIN_POSITIVE,
            f32::MAX,
            f32::MIN,
            f32::EPSILON,
            1.0e-45,
            16_777_217.0,
            0.333_333_34,
        ];
        for _ in 0..200_000 {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let x = f32::from_bits(state as u32);
            if x.is_finite() {
                values.push(x);
            }
        }
        let numbers: Vec<Num> = values.iter().map(|x| Num::from_f32(*x)).collect();
        let text = serde_json::to_string(&numbers).unwrap();
        let back: Vec<Num> = serde_json::from_str(&text).unwrap();
        for (x, n) in values.iter().zip(&back) {
            assert_eq!(n.to_f32().to_bits(), x.to_bits(), "{x:e}");
        }
        assert_eq!(serde_json::to_string(&Num::from_f32(0.8)).unwrap(), "0.8");
    }

    #[test]
    fn f64_bit_patterns_survive_json() {
        let mut state = 0x1234_5678_9ABC_DEF1u64;
        let mut values = vec![0.0f64, -0.0, 1.0, 0.1, f64::MIN_POSITIVE, f64::MAX, 5e-324];
        for _ in 0..200_000 {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let x = f64::from_bits(state);
            if x.is_finite() {
                values.push(x);
            }
        }
        let numbers: Vec<Num> = values.iter().map(|x| Num(*x)).collect();
        let back: Vec<Num> =
            serde_json::from_str(&serde_json::to_string(&numbers).unwrap()).unwrap();
        for (x, n) in values.iter().zip(&back) {
            assert_eq!(n.0.to_bits(), x.to_bits(), "{x:e}");
        }
    }

    #[test]
    fn values_json_cannot_spell_are_written_as_strings() {
        let numbers = [Num(f64::NAN), Num(f64::INFINITY), Num(f64::NEG_INFINITY)];
        let text = serde_json::to_string(&numbers).unwrap();
        assert_eq!(text, r#"["NaN","Infinity","-Infinity"]"#);
        let back: [Num; 3] = serde_json::from_str(&text).unwrap();
        assert!(back[0].0.is_nan());
        assert_eq!(back[1].0, f64::INFINITY);
        assert_eq!(back[2].0, f64::NEG_INFINITY);
        assert!(serde_json::from_str::<Num>("\"big\"").is_err());
        assert_eq!(serde_json::from_str::<Num>("3").unwrap(), Num(3.0));
    }
}

//! Lenient access to Quill's JSON. Quill 2.x writes many integers as strings and
//! other writers do not, so every accessor accepts both.

use serde_json::Value;

pub(crate) fn as_i64(v: &Value) -> Option<i64> {
    match v {
        Value::Number(n) => n.as_i64().or_else(|| n.as_f64().map(|f| f as i64)),
        Value::String(s) => {
            let s = s.trim();
            s.parse::<i64>()
                .ok()
                .or_else(|| s.parse::<f64>().ok().map(|f| f as i64))
        }
        Value::Bool(b) => Some(*b as i64),
        _ => None,
    }
}

pub(crate) fn as_f64(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse::<f64>().ok(),
        _ => None,
    }
}

pub(crate) fn as_bool(v: &Value) -> Option<bool> {
    match v {
        Value::Bool(b) => Some(*b),
        Value::Number(n) => n.as_f64().map(|f| f != 0.0),
        Value::String(s) => match s.trim().to_ascii_lowercase().as_str() {
            "true" | "1" => Some(true),
            "false" | "0" => Some(false),
            _ => None,
        },
        _ => None,
    }
}

pub(crate) fn get_i64(obj: &Value, key: &str) -> Option<i64> {
    obj.get(key).and_then(as_i64)
}

pub(crate) fn get_f64(obj: &Value, key: &str) -> Option<f64> {
    obj.get(key).and_then(as_f64)
}

pub(crate) fn get_bool(obj: &Value, key: &str) -> Option<bool> {
    obj.get(key).and_then(as_bool)
}

pub(crate) fn get_str<'a>(obj: &'a Value, key: &str) -> Option<&'a str> {
    obj.get(key).and_then(Value::as_str)
}

/// Reads an array of numbers of exactly `N` elements.
pub(crate) fn floats<const N: usize>(v: &Value) -> Option<[f64; N]> {
    let array = v.as_array()?;
    if array.len() != N {
        return None;
    }
    let mut out = [0.0; N];
    for (slot, item) in out.iter_mut().zip(array) {
        *slot = as_f64(item)?;
    }
    Some(out)
}

/// Parses a `DataFileOffset`: hexadecimal, zero-padded in newer files and bare in old ones.
pub(crate) fn file_offset(v: &Value) -> Option<u64> {
    match v {
        Value::String(s) => u64::from_str_radix(s.trim().trim_start_matches("0x"), 16).ok(),
        Value::Number(n) => n.as_u64(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn integers_are_read_from_numbers_and_strings() {
        assert_eq!(as_i64(&json!(525)), Some(525));
        assert_eq!(as_i64(&json!("45360000")), Some(45_360_000));
        assert_eq!(as_i64(&json!(" 12 ")), Some(12));
        assert_eq!(as_i64(&json!(3.0)), Some(3));
        assert_eq!(as_i64(&json!([1])), None);
    }

    #[test]
    fn offsets_are_hexadecimal() {
        assert_eq!(file_offset(&json!("00000000000c001c")), Some(0xc001c));
        assert_eq!(file_offset(&json!("8")), Some(8));
        assert_eq!(file_offset(&json!("1F")), Some(31));
        assert_eq!(file_offset(&json!("zz")), None);
    }

    #[test]
    fn fixed_size_arrays() {
        assert_eq!(floats::<3>(&json!([0.5, 1, "2"])), Some([0.5, 1.0, 2.0]));
        assert_eq!(floats::<3>(&json!([0.5, 1])), None);
    }
}

//! Small helpers for the JSON side of commands and saved state.

use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{Map, Value};

/// Writes the fields of `patch` over `target`, descending into objects so that a
/// patch can change one nested field and leave its siblings alone.
pub fn merge(target: &mut Value, patch: &Value) {
    match (target, patch) {
        (Value::Object(target), Value::Object(patch)) => {
            for (key, value) in patch {
                match target.get_mut(key) {
                    Some(existing) if existing.is_object() && value.is_object() => {
                        merge(existing, value)
                    }
                    _ => {
                        target.insert(key.clone(), value.clone());
                    }
                }
            }
        }
        (target, patch) => *target = patch.clone(),
    }
}

/// Returns `value` with the fields named in `patch` replaced. Fails, naming the
/// problem, when a field does not exist or has the wrong type.
pub fn patched<T: Serialize + DeserializeOwned>(
    value: &T,
    patch: &Map<String, Value>,
) -> Result<T, String> {
    let mut json = serde_json::to_value(value).map_err(|e| e.to_string())?;
    check_known(&json, patch, "")?;
    merge(&mut json, &Value::Object(patch.clone()));
    serde_json::from_value(json).map_err(|e| e.to_string())
}

fn check_known(target: &Value, patch: &Map<String, Value>, path: &str) -> Result<(), String> {
    let Value::Object(fields) = target else {
        return Ok(());
    };
    for (key, value) in patch {
        let Some(existing) = fields.get(key) else {
            return Err(format!("there is no setting named \"{path}{key}\""));
        };
        if let (Value::Object(_), Value::Object(inner)) = (existing, value) {
            check_known(existing, inner, &format!("{path}{key}."))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;
    use serde_json::json;

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct Inner {
        size: f32,
        on: bool,
    }

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct Outer {
        name: String,
        inner: Inner,
    }

    fn sample() -> Outer {
        Outer {
            name: "a".into(),
            inner: Inner {
                size: 1.0,
                on: false,
            },
        }
    }

    fn object(value: Value) -> Map<String, Value> {
        value.as_object().unwrap().clone()
    }

    #[test]
    fn a_patch_changes_only_the_fields_it_names() {
        let out = patched(&sample(), &object(json!({"inner": {"on": true}}))).unwrap();
        assert_eq!(
            out,
            Outer {
                name: "a".into(),
                inner: Inner {
                    size: 1.0,
                    on: true
                }
            }
        );
    }

    #[test]
    fn unknown_fields_and_wrong_types_are_reported() {
        let unknown = patched(&sample(), &object(json!({"inner": {"sise": 2.0}}))).unwrap_err();
        assert!(unknown.contains("inner.sise"), "{unknown}");
        assert!(patched(&sample(), &object(json!({"name": 3}))).is_err());
    }
}

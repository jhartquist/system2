use crate::InvalidJson;
use serde::Serialize;
use serde_json::Value;

pub(crate) fn validate(value: impl Serialize) -> Result<Value, InvalidJson> {
    let value = serde_json::to_value(value).map_err(InvalidJson::Serialize)?;
    match &value {
        Value::Number(_) | Value::Bool(_) => Err(InvalidJson::Shape),
        Value::Null => Err(InvalidJson::Empty),
        Value::String(text) if text.trim().is_empty() => Err(InvalidJson::Empty),
        Value::Array(items) if items.is_empty() => Err(InvalidJson::Empty),
        Value::Object(fields) if fields.is_empty() => Err(InvalidJson::Empty),
        _ => Ok(value),
    }
}

#[cfg(test)]
mod tests {
    use super::validate;
    use crate::InvalidJson;
    use serde_json::json;
    use std::collections::BTreeMap;

    #[test]
    fn validate_takes_text_an_object_or_an_array() {
        let cases = [
            json!("Is it spam?"),
            json!({ "question": "Is it spam?" }),
            json!(["Is it spam?"]),
        ];
        for value in cases {
            assert_eq!(validate(&value).unwrap(), value);
        }
    }

    #[test]
    fn validate_refuses_an_empty_value() {
        let cases = [json!(""), json!(" \n\t"), json!({}), json!([]), json!(null)];
        for value in cases {
            let error = validate(&value).unwrap_err();
            assert!(matches!(error, InvalidJson::Empty), "{value}");
            assert_eq!(error.to_string(), "must not be empty");
        }
    }

    #[test]
    fn validate_refuses_a_number_or_a_bool() {
        for value in [json!(42), json!(true)] {
            let error = validate(&value).unwrap_err();
            assert!(matches!(error, InvalidJson::Shape), "{value}");
            assert_eq!(error.to_string(), "must be text, an object, or an array");
        }
    }

    #[test]
    fn validate_refuses_a_value_that_does_not_serialize() {
        let pairs = BTreeMap::from([((1, 2), "a pair as a key")]);

        let error = validate(pairs).unwrap_err();

        assert!(matches!(error, InvalidJson::Serialize(_)));
        assert_eq!(
            error.to_string(),
            "must serialize to JSON: key must be a string"
        );
    }
}

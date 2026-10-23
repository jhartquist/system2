use crate::InvalidJson;
use serde::Serialize;
use serde_json::value::{RawValue, to_raw_value};

// raw json keeps the order that serde writes, which a `Value` sorts
pub(crate) fn validate(value: impl Serialize) -> Result<Box<RawValue>, InvalidJson> {
    let json = to_raw_value(&value).map_err(InvalidJson::Serialize)?;

    // the text is valid json, so its first character gives its type
    let text = json.get().trim();
    let empty = match text.chars().next() {
        Some('"') => serde_json::from_str::<String>(text).is_ok_and(|text| text.trim().is_empty()),
        Some('{' | '[') => text[1..text.len() - 1].trim().is_empty(),
        Some('n') => true,
        _ => return Err(InvalidJson::Shape),
    };
    if empty {
        return Err(InvalidJson::Empty);
    }
    Ok(json)
}

pub(crate) fn validate_optional(
    value: impl Serialize,
) -> Result<Option<Box<RawValue>>, InvalidJson> {
    let json = to_raw_value(&value).map_err(InvalidJson::Serialize)?;
    if json.get().trim() == "null" {
        return Ok(None);
    }
    validate(json).map(Some)
}

#[cfg(test)]
mod tests {
    use super::{validate, validate_optional};
    use crate::InvalidJson;
    use serde::Serialize;
    use serde_json::value::RawValue;
    use std::collections::BTreeMap;

    // one type for each test of `validate`, so coverage sees all its checks together
    fn raw(text: &str) -> Box<RawValue> {
        RawValue::from_string(text.to_owned()).unwrap()
    }

    #[test]
    fn validate_takes_text_an_object_or_an_array() {
        let cases = [
            r#""Is it spam?""#,
            r#"{"question":"Is it spam?"}"#,
            r#"["Is it spam?"]"#,
        ];
        for text in cases {
            assert_eq!(validate(raw(text)).unwrap().get(), text);
        }
    }

    #[test]
    fn validate_keeps_the_order_of_the_fields() {
        #[derive(Serialize)]
        struct Change {
            diff: &'static str,
            description: &'static str,
        }
        let change = Change {
            diff: "+ fn main() {}",
            description: "Add main.",
        };

        assert_eq!(
            validate(change).unwrap().get(),
            r#"{"diff":"+ fn main() {}","description":"Add main."}"#
        );
    }

    #[test]
    fn validate_refuses_an_empty_value() {
        let cases = [r#""""#, r#"" \n\t""#, "{}", "[ ]", " { \n } ", "null"];
        for text in cases {
            let error = validate(raw(text)).unwrap_err();
            assert!(matches!(error, InvalidJson::Empty), "{text}");
            assert_eq!(error.to_string(), "must not be empty");
        }
    }

    #[test]
    fn validate_refuses_a_number_or_a_bool() {
        for text in ["42", "-1", "true", "false"] {
            let error = validate(raw(text)).unwrap_err();
            assert!(matches!(error, InvalidJson::Shape), "{text}");
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

    #[test]
    fn validate_optional_takes_null_as_no_value() {
        assert!(validate_optional(()).unwrap().is_none());
        assert!(validate_optional(None::<&str>).unwrap().is_none());
        assert_eq!(
            validate_optional("Ask for a fix.").unwrap().unwrap().get(),
            r#""Ask for a fix.""#
        );
    }

    #[test]
    fn validate_optional_refuses_what_validate_refuses() {
        let cases = [
            (validate_optional("").unwrap_err(), "must not be empty"),
            (
                validate_optional(1).unwrap_err(),
                "must be text, an object, or an array",
            ),
            (
                validate_optional(BTreeMap::from([((1, 2), "")])).unwrap_err(),
                "must serialize to JSON: key must be a string",
            ),
        ];
        for (error, message) in cases {
            assert_eq!(error.to_string(), message);
        }
    }
}

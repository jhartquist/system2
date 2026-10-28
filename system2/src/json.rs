use serde::ser::Error;
use serde::{Serialize, Serializer};
use serde_json::value::{RawValue, to_raw_value};
use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
use std::fmt;
use std::rc::Rc;
use std::sync::Arc;

// the instructions or a criterion, after a macro or a constructor validates it: text, an
// object, or an array, and never empty
#[doc(hidden)]
#[derive(Debug, Clone)]
pub enum Json {
    // the value of a json string, such as `Is it "spam"?`, which goes out as `"Is it \"spam\"?"`
    String(Cow<'static, str>),
    // json text, such as `{"policy":"No secrets."}`, which goes out with no change
    Raw(Cow<'static, str>),
}

impl From<Box<RawValue>> for Json {
    fn from(json: Box<RawValue>) -> Self {
        Json::Raw(Cow::Owned(Box::<str>::from(json).into_string()))
    }
}

impl Serialize for Json {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Json::String(text) => serializer.serialize_str(text),
            Json::Raw(json) => serde_json::from_str::<&RawValue>(json)
                .map_err(S::Error::custom)?
                .serialize(serializer),
        }
    }
}

#[doc(hidden)]
#[diagnostic::on_unimplemented(
    message = "`{Self}` can fail to serialize, so a macro cannot prove the question valid",
    label = "can fail to serialize",
    note = "use a constructor such as `Noul::new(json!({{ … }}))?`, or put the value in the state"
)]
pub trait AlwaysSerialize: Serialize {}

impl AlwaysSerialize for str {}
impl AlwaysSerialize for String {}
impl AlwaysSerialize for char {}
impl AlwaysSerialize for bool {}
impl AlwaysSerialize for () {}
impl AlwaysSerialize for serde_json::Value {}
impl<T: AlwaysSerialize + ?Sized> AlwaysSerialize for &T {}
impl<T: AlwaysSerialize + ?Sized> AlwaysSerialize for Box<T> {}
impl<T: AlwaysSerialize + ToOwned + ?Sized> AlwaysSerialize for Cow<'_, T> {}
impl<T: AlwaysSerialize> AlwaysSerialize for Option<T> {}
impl<T: AlwaysSerialize> AlwaysSerialize for [T] {}
impl<T: AlwaysSerialize> AlwaysSerialize for Vec<T> {}
impl<T: AlwaysSerialize> AlwaysSerialize for VecDeque<T> {}
impl<T: AlwaysSerialize> AlwaysSerialize for BTreeSet<T> {}
impl<T: AlwaysSerialize, S> AlwaysSerialize for HashSet<T, S> {}

// serde serializes an array of up to 32 items, and an `Rc` or an `Arc` only with its `rc` feature
impl<T: AlwaysSerialize, const N: usize> AlwaysSerialize for [T; N] where [T; N]: Serialize {}
impl<T: AlwaysSerialize + ?Sized> AlwaysSerialize for Rc<T> where Rc<T>: Serialize {}
impl<T: AlwaysSerialize + ?Sized> AlwaysSerialize for Arc<T> where Arc<T>: Serialize {}

// a json key is always text
impl<T: AlwaysSerialize> AlwaysSerialize for BTreeMap<String, T> {}
impl<T: AlwaysSerialize> AlwaysSerialize for BTreeMap<&str, T> {}
impl<T: AlwaysSerialize, S> AlwaysSerialize for HashMap<String, T, S> {}
impl<T: AlwaysSerialize, S> AlwaysSerialize for HashMap<&str, T, S> {}

macro_rules! tuple {
    ($($item:ident),+) => { impl<$($item: AlwaysSerialize),+> AlwaysSerialize for ($($item,)+) {} };
}

tuple!(A);
tuple!(A, B);
tuple!(A, B, C);
tuple!(A, B, C, D);
tuple!(A, B, C, D, E);
tuple!(A, B, C, D, E, F);
tuple!(A, B, C, D, E, F, G);
tuple!(A, B, C, D, E, F, G, H);
tuple!(A, B, C, D, E, F, G, H, I);
tuple!(A, B, C, D, E, F, G, H, I, J);
tuple!(A, B, C, D, E, F, G, H, I, J, K);
tuple!(A, B, C, D, E, F, G, H, I, J, K, L);
tuple!(A, B, C, D, E, F, G, H, I, J, K, L, M);
tuple!(A, B, C, D, E, F, G, H, I, J, K, L, M, N);
tuple!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O);
tuple!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P);

macro_rules! numbers {
    ($($number:ty),*) => { $(impl AlwaysSerialize for $number {})* };
}

numbers!(
    i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize, f32, f64
);

#[doc(hidden)]
#[expect(clippy::expect_used, reason = "the value always serializes")]
#[must_use]
pub fn to_json(value: &impl AlwaysSerialize) -> String {
    serde_json::to_string(value).expect("the value always serializes")
}

// a literal with no placeholder, such as "a {{b}}", gives "a {b}", also in a const
#[doc(hidden)]
#[expect(
    clippy::expect_used,
    reason = "the macro gives only a literal with no placeholder"
)]
#[must_use]
pub const fn unescaped(arguments: fmt::Arguments<'_>) -> &'static str {
    arguments.as_str().expect("a literal with no placeholder")
}

// `str::trim` is not const, so this matches the utf-8 bytes of each whitespace char: tab to
// carriage return, space, U+0085, U+00A0, U+1680, U+2000 to U+200A, U+2028, U+2029, U+202F,
// U+205F, and U+3000
#[doc(hidden)]
#[must_use]
pub const fn is_blank(text: &str) -> bool {
    let mut bytes = text.as_bytes();
    loop {
        bytes = match bytes {
            [] => return true,
            [b'\t'..=b'\r' | b' ', rest @ ..]
            | [0xC2, 0x85 | 0xA0, rest @ ..]
            | [0xE1, 0x9A, 0x80, rest @ ..]
            | [0xE2, 0x80, 0x80..=0x8A | 0xA8 | 0xA9 | 0xAF, rest @ ..]
            | [0xE2, 0x81, 0x9F, rest @ ..]
            | [0xE3, 0x80, 0x80, rest @ ..] => rest,
            _ => return false,
        };
    }
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum InvalidJson {
    #[error("must serialize to JSON: {0}")]
    Serialize(serde_json::Error),
    #[error("must be text, an object, or an array")]
    Shape,
    #[error("must not be empty")]
    Empty,
}

// raw json keeps the order that serde writes, which a `Value` sorts
pub(crate) fn validate(value: impl Serialize) -> Result<Box<RawValue>, InvalidJson> {
    let json = to_raw_value(&value).map_err(InvalidJson::Serialize)?;

    // the text is valid json, so its first character gives its type
    let text = json.get().trim();
    let empty = match text.chars().next() {
        Some('"') => serde_json::from_str::<String>(text).is_ok_and(|text| is_blank(&text)),
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
    use super::{Json, is_blank, to_json, unescaped, validate, validate_optional};
    use crate::InvalidJson;
    use serde::Serialize;
    use serde_json::json;
    use serde_json::value::RawValue;
    use std::borrow::Cow;
    use std::collections::BTreeMap;

    #[test]
    fn serialize_quotes_only_a_string() {
        let cases = [
            (
                Json::String(Cow::Borrowed(r#"Is {"a": 1} JSON?"#)),
                r#""Is {\"a\": 1} JSON?""#,
            ),
            (Json::Raw(Cow::Borrowed(r#"{"a":1}"#)), r#"{"a":1}"#),
            (
                Json::from(RawValue::from_string(r#"["a"]"#.to_owned()).unwrap()),
                r#"["a"]"#,
            ),
        ];
        for (json, text) in cases {
            assert_eq!(serde_json::to_string(&json).unwrap(), text, "{json:?}");
        }
    }

    #[test]
    fn to_json_gives_the_json_of_a_value() {
        let value = (
            vec![Some("a"), None],
            Box::new('b'),
            Cow::Borrowed("c"),
            BTreeMap::from([("d", 1)]),
            (),
            [1.5],
        );
        assert_eq!(
            to_json(&value),
            r#"[["a",null],"b","c",{"d":1},null,[1.5]]"#
        );
    }

    #[test]
    fn unescaped_gives_each_brace_one_time() {
        assert_eq!(
            unescaped(format_args!("Is `{{}}` a block?")),
            "Is `{}` a block?"
        );
    }

    #[test]
    fn is_blank_takes_only_whitespace_as_blank() {
        let cases = [
            ("", true),
            (" ", true),
            (" \t\n\r", true),
            ("\u{a0}", true),
            ("\u{2003}\u{3000}", true),
            (" a ", false),
            ("é", false),
            ("日本", false),
            ("🦀", false),
            ("\u{3000}a", false),
            // these look empty, but unicode has no whitespace property for them
            ("\u{200b}", false),
            ("\u{feff}", false),
        ];
        for (text, blank) in cases {
            assert_eq!(is_blank(text), blank, "{text:?}");
        }
    }

    #[test]
    fn is_blank_agrees_with_is_whitespace_for_each_char() {
        for character in char::MIN..=char::MAX {
            let text = character.to_string();
            assert_eq!(is_blank(&text), character.is_whitespace(), "{character:?}");
        }
    }

    #[test]
    fn validate_refuses_text_of_unicode_whitespace() {
        let error = validate(json!("\u{a0}\u{2003}")).unwrap_err();

        assert!(matches!(error, InvalidJson::Empty));
    }

    // one type for each test of `validate`, so coverage sees all its rules together
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

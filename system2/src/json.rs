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

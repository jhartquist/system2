use crate::{InvalidJson, json};
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone)]
#[expect(dead_code, reason = "the request reads them")]
pub struct Noul {
    instructions: Value,
    criteria: Criteria,
}

#[derive(Debug, Clone, Default)]
#[expect(dead_code, reason = "the request reads them")]
struct Criteria {
    r#true: Option<Value>,
    r#false: Option<Value>,
}

impl Noul {
    pub fn new(instructions: impl Serialize) -> Result<Self, InvalidNoul> {
        let instructions = json::validate(instructions).map_err(InvalidNoul::Instructions)?;
        let criteria = Criteria::default();
        Ok(Noul {
            instructions,
            criteria,
        })
    }

    pub fn with_criteria(
        instructions: impl Serialize,
        criteria: impl Serialize,
    ) -> Result<Self, InvalidNoul> {
        let instructions = json::validate(instructions).map_err(InvalidNoul::Instructions)?;
        let criteria = Criteria::from_data(criteria)?;
        Ok(Noul {
            instructions,
            criteria,
        })
    }
}

impl Criteria {
    fn from_data(criteria: impl Serialize) -> Result<Self, InvalidNoul> {
        let Ok(Value::Object(mut fields)) = serde_json::to_value(criteria) else {
            return Err(InvalidNoul::Criteria);
        };
        if fields.is_empty() || fields.keys().any(|key| key != "true" && key != "false") {
            return Err(InvalidNoul::Criteria);
        }
        let mut take = |key| fields.remove(key).map(json::validate).transpose();
        let r#true = take("true").map_err(InvalidNoul::True)?;
        let r#false = take("false").map_err(InvalidNoul::False)?;
        Ok(Criteria { r#true, r#false })
    }
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum InvalidNoul {
    #[error("the instructions {0}")]
    Instructions(InvalidJson),
    #[error("the criteria must be an object with the key `true`, the key `false`, or both")]
    Criteria,
    #[error("the `true` criterion {0}")]
    True(InvalidJson),
    #[error("the `false` criterion {0}")]
    False(InvalidJson),
}

#[derive(Debug, Clone)]
#[expect(dead_code, reason = "read when the methods have bodies")]
pub struct NoulAnswer {
    probability: f64,
}

impl NoulAnswer {
    #[must_use]
    pub fn probability(&self) -> f64 {
        todo!()
    }

    #[must_use]
    pub fn is_likely(&self) -> bool {
        todo!()
    }
}

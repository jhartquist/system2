use crate::InvalidJson;
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone)]
#[expect(dead_code, reason = "read when the constructors have bodies")]
pub struct Noul {
    instructions: Value,
    criteria: Criteria,
}

#[derive(Debug, Clone)]
#[expect(dead_code, reason = "read when the constructors have bodies")]
struct Criteria {
    r#true: Option<Value>,
    r#false: Option<Value>,
}

impl Noul {
    pub fn new(instructions: impl Serialize) -> Result<Self, InvalidNoul> {
        let _ = instructions;
        todo!()
    }

    pub fn with_criteria(
        instructions: impl Serialize,
        criteria: impl Serialize,
    ) -> Result<Self, InvalidNoul> {
        let _ = (instructions, criteria);
        todo!()
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

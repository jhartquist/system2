use crate::{InvalidJson, json};
use serde::Serialize;
use serde_json::{Value, json};

#[derive(Debug, Clone)]
pub struct Noul {
    instructions: Value,
    criteria: Option<Criteria>,
}

#[derive(Debug, Clone, Serialize)]
struct Criteria {
    #[serde(skip_serializing_if = "Option::is_none")]
    r#true: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    r#false: Option<Value>,
}

impl Noul {
    pub fn new(instructions: impl Serialize) -> Result<Self, InvalidNoul> {
        let instructions = json::validate(instructions).map_err(InvalidNoul::Instructions)?;
        let criteria = None;
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
        let criteria = Some(Criteria::new(criteria)?);
        Ok(Noul {
            instructions,
            criteria,
        })
    }

    pub(crate) fn to_json(&self) -> Value {
        let Noul {
            instructions,
            criteria,
        } = self;
        let mut question = json!({ "type": "noul", "instructions": instructions });
        if let Some(criteria) = criteria {
            question["criteria"] = json!(criteria);
        }
        question
    }
}

impl Criteria {
    fn new(criteria: impl Serialize) -> Result<Self, InvalidNoul> {
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
pub struct NoulAnswer {
    probability: f64,
}

impl NoulAnswer {
    pub(crate) fn new(probability: f64) -> Self {
        NoulAnswer { probability }
    }

    #[must_use]
    pub fn probability(&self) -> f64 {
        self.probability
    }

    #[must_use]
    pub fn is_likely(&self) -> bool {
        self.probability >= 0.5
    }
}

#[cfg(test)]
mod tests {
    use super::{InvalidNoul, Noul, NoulAnswer};
    use crate::InvalidJson;
    use serde_json::json;
    use std::collections::BTreeMap;

    #[test]
    fn new_refuses_empty_instructions() {
        let error = Noul::new("").unwrap_err();

        assert!(matches!(
            error,
            InvalidNoul::Instructions(InvalidJson::Empty)
        ));
        assert_eq!(error.to_string(), "the instructions must not be empty");
    }

    #[test]
    fn with_criteria_refuses_empty_instructions() {
        let error = Noul::with_criteria("", json!({ "true": "It sells." })).unwrap_err();

        assert!(matches!(
            error,
            InvalidNoul::Instructions(InvalidJson::Empty)
        ));
    }

    #[test]
    fn with_criteria_refuses_criteria_of_another_shape() {
        let cases = [
            json!("It sells a product."),
            json!({}),
            json!({ "ture": "It sells a product." }),
            json!({ "true": "It sells a product.", "maybe": "It links out." }),
        ];
        for criteria in cases {
            let error = Noul::with_criteria("Is it spam?", &criteria).unwrap_err();
            assert!(matches!(error, InvalidNoul::Criteria), "{criteria}");
            assert_eq!(
                error.to_string(),
                "the criteria must be an object with the key `true`, the key `false`, or both"
            );
        }
    }

    #[test]
    fn with_criteria_refuses_criteria_that_do_not_serialize() {
        let pairs = BTreeMap::from([((1, 2), "a pair as a key")]);
        let error = Noul::with_criteria("Is it spam?", pairs).unwrap_err();
        assert!(matches!(error, InvalidNoul::Criteria));

        let too_large = BTreeMap::from([("true", u128::MAX)]);
        let error = Noul::with_criteria("Is it spam?", too_large).unwrap_err();
        assert!(matches!(error, InvalidNoul::Criteria));
    }

    #[test]
    fn with_criteria_names_the_criterion_in_its_error() {
        let error = Noul::with_criteria("Is it spam?", json!({ "true": "" })).unwrap_err();
        assert!(matches!(error, InvalidNoul::True(InvalidJson::Empty)));
        assert_eq!(error.to_string(), "the `true` criterion must not be empty");

        let error = Noul::with_criteria("Is it spam?", json!({ "false": 0 })).unwrap_err();
        assert!(matches!(error, InvalidNoul::False(InvalidJson::Shape)));
        assert_eq!(
            error.to_string(),
            "the `false` criterion must be text, an object, or an array"
        );
    }

    #[test]
    fn to_json_has_the_type_and_the_instructions() {
        let noul = Noul::new("Is it spam?").unwrap();

        assert_eq!(
            noul.to_json(),
            json!({ "type": "noul", "instructions": "Is it spam?" })
        );
    }

    #[test]
    fn to_json_has_only_the_criteria_of_the_noul() {
        let cases = [
            json!({ "true": "It sells a product." }),
            json!({ "false": { "covers": "questions and replies" } }),
            json!({ "true": "It sells a product.", "false": "It is a reply." }),
        ];
        for criteria in cases {
            let noul = Noul::with_criteria("Is it spam?", &criteria).unwrap();
            assert_eq!(
                noul.to_json(),
                json!({ "type": "noul", "instructions": "Is it spam?", "criteria": criteria })
            );
        }
    }

    #[test]
    fn is_likely_at_one_half_or_more() {
        assert!(NoulAnswer::new(0.5).is_likely());
        assert!(!NoulAnswer::new(0.49).is_likely());
    }
}

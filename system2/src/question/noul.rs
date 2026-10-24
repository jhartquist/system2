use crate::ask::{self, private::Sealed};
use crate::{Ask, InvalidJson, json};
use serde::ser::SerializeStruct;
use serde::{Deserialize, Serialize, Serializer};
use serde_json::Value;
use serde_json::value::{RawValue, to_raw_value};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct Noul {
    instructions: Box<RawValue>,
    criteria: Option<Criteria>,
}

#[derive(Debug, Clone, Serialize)]
struct Criteria {
    #[serde(skip_serializing_if = "Option::is_none")]
    r#true: Option<Box<RawValue>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    r#false: Option<Box<RawValue>>,
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
}

impl Criteria {
    fn new(criteria: impl Serialize) -> Result<Self, InvalidNoul> {
        let mut fields: HashMap<String, Box<RawValue>> = to_raw_value(&criteria)
            .ok()
            .and_then(|json| serde_json::from_str(json.get()).ok())
            .filter(|fields: &HashMap<_, _>| {
                !fields.is_empty() && fields.keys().all(|key| key == "true" || key == "false")
            })
            .ok_or(InvalidNoul::Criteria)?;
        let mut take = |key| fields.remove(key).map(json::validate).transpose();
        let r#true = take("true").map_err(InvalidNoul::True)?;
        let r#false = take("false").map_err(InvalidNoul::False)?;
        Ok(Criteria { r#true, r#false })
    }
}

impl Ask for Noul {
    type Answer = NoulAnswer;
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
    question: Noul,
    probability: f64,
}

impl NoulAnswer {
    #[must_use]
    pub fn question(&self) -> &Noul {
        &self.question
    }

    #[must_use]
    pub fn into_question(self) -> Noul {
        self.question
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

impl Serialize for NoulAnswer {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut answer = serializer.serialize_struct("NoulAnswer", 1)?;
        answer.serialize_field("probability", &self.probability)?;
        answer.end()
    }
}

#[derive(Serialize)]
#[serde(tag = "type", rename = "noul")]
struct Question<'a> {
    instructions: &'a RawValue,
    #[serde(skip_serializing_if = "Option::is_none")]
    criteria: Option<&'a Criteria>,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum Answer {
    Noul { noul: f64 },
}

impl Sealed for Noul {
    fn to_json(&self) -> impl Serialize {
        let instructions = &self.instructions;
        let criteria = self.criteria.as_ref();
        Question {
            instructions,
            criteria,
        }
    }

    fn into_answer(self, json: Value) -> Result<<Self as Ask>::Answer, String> {
        let Answer::Noul { noul: probability } = ask::from_json(json)?;
        let question = self;
        Ok(NoulAnswer {
            question,
            probability,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{InvalidNoul, Noul, NoulAnswer};
    use crate::InvalidJson;
    use crate::ask::private::Sealed;
    use serde_json::{Value, json};
    use std::collections::BTreeMap;

    fn serialized(noul: &Noul) -> Value {
        serde_json::to_value(noul.to_json()).unwrap()
    }

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
    fn new_refuses_a_number_or_a_bool() {
        for instructions in [json!(42), json!(0.5), json!(true)] {
            let error = Noul::new(&instructions).unwrap_err();
            assert!(
                matches!(error, InvalidNoul::Instructions(InvalidJson::Shape)),
                "{instructions}"
            );
            assert_eq!(
                error.to_string(),
                "the instructions must be text, an object, or an array"
            );
        }
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
            serialized(&noul),
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
                serialized(&noul),
                json!({ "type": "noul", "instructions": "Is it spam?", "criteria": criteria }),
                "{criteria}"
            );
        }
    }

    #[test]
    fn into_answer_takes_the_probability_and_keeps_the_question() {
        let noul = Noul::new("Is it spam?").unwrap();

        let answer = noul
            .into_answer(json!({ "type": "noul", "noul": 0.92 }))
            .unwrap();

        assert_eq!(answer.probability(), 0.92);
        assert_eq!(answer.question().instructions.get(), r#""Is it spam?""#);
        assert_eq!(
            answer.into_question().instructions.get(),
            r#""Is it spam?""#
        );
    }

    #[test]
    fn into_answer_refuses_another_type_of_answer() {
        let noul = Noul::new("Is it spam?").unwrap();

        let error = noul
            .into_answer(json!({ "type": "box", "confidence": 0.9 }))
            .unwrap_err();

        assert_eq!(
            error,
            "must answer each question: unknown variant `box`, expected `noul`"
        );
    }

    #[test]
    fn serialize_gives_the_probability() {
        let noul = Noul::new("Is it spam?").unwrap();
        let answer = noul
            .into_answer(json!({ "type": "noul", "noul": 0.92 }))
            .unwrap();

        assert_eq!(
            serde_json::to_string(&answer).unwrap(),
            r#"{"probability":0.92}"#
        );
    }

    #[test]
    fn is_likely_at_one_half_or_more() {
        let answer = |probability| NoulAnswer {
            question: Noul::new("Is it spam?").unwrap(),
            probability,
        };

        assert!(answer(0.5).is_likely());
        assert!(!answer(0.49).is_likely());
    }
}

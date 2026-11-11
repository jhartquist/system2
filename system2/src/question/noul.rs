use crate::ask::wire::{RawAnswerSet, RawQuestionSet, Sealed, validate_range};
use crate::json::Json;
use crate::{InvalidAnswer, InvalidJson, QuestionSet, Tagged, json};
use serde::ser::SerializeStruct;
use serde::{Deserialize, Serialize, Serializer};
use serde_json::value::{RawValue, to_raw_value};
use std::collections::HashMap;

/// A yes-or-no question.
///
/// A Noul has instructions, plus optional criteria that spell out what a yes and a no mean.
/// Asking it gives you a [`NoulAnswer`] with the probability of yes. Use [`noul!`](crate::noul)
/// when you know the question at compile time, so the compiler checks it, and [`Noul::new`] when
/// it comes from data.
///
/// # Examples
///
/// The smallest Noul is a line of text:
///
/// ```
/// use system2::Noul;
///
/// let urgent = Noul::new("Is the ticket urgent?")?;
/// # Ok::<(), system2::Error>(())
/// ```
///
/// The text can come from data:
///
/// ```
/// # use system2::Noul;
/// let product = "the mobile app".to_string();
/// let about = Noul::new(format!("Is the ticket about {product}?"))?;
/// # Ok::<(), system2::Error>(())
/// ```
///
/// The instructions can also be JSON, which gives each part a name:
///
/// ```
/// use serde_json::json;
/// # use system2::Noul;
///
/// let policy = "A reply never promises a date.".to_string();
/// let obeys = Noul::new(json!({
///     "question": "Does the reply obey the policy?",
///     "policy": policy,
/// }))?;
/// # Ok::<(), system2::Error>(())
/// ```
///
/// A struct that derives `Serialize` works the same way:
///
/// ```
/// use serde::Serialize;
/// # use system2::Noul;
///
/// #[derive(Serialize)]
/// struct Rule {
///     question: String,
///     policy: String,
/// }
///
/// let rule = Rule {
///     question: "Does the reply obey the policy?".to_string(),
///     policy: "A reply never promises a date.".to_string(),
/// };
/// let obeys = Noul::new(rule)?;
/// # Ok::<(), system2::Error>(())
/// ```
///
/// Criteria spell out what a yes and a no mean:
///
/// ```
/// use serde_json::json;
/// # use system2::Noul;
///
/// let refund = Noul::with_criteria(
///     "Does the customer ask for a refund?",
///     json!({
///         "true": "The customer wants the money back.",
///         "false": "The customer wants no money.",
///     }),
/// )?;
/// # Ok::<(), system2::Error>(())
/// ```
///
/// You can give only one of them, and they can come from a map:
///
/// ```
/// use std::collections::BTreeMap;
/// # use system2::Noul;
///
/// let criteria = BTreeMap::from([("true", "The site is down for the customer.")]);
/// let urgent = Noul::with_criteria("Is the ticket urgent?", criteria)?;
/// # Ok::<(), system2::Error>(())
/// ```
///
/// [`noul!`](crate::noul) takes the same forms and checks them at compile time. A question with
/// no variables in it can be a constant:
///
/// ```no_run
/// use system2::prelude::*;
/// use system2::Noul;
///
/// const URGENT: Noul = noul!("Is the ticket urgent?");
/// const REFUND: Noul = noul!("Does the customer ask for a refund?", {
///     "true": "The customer wants the money back.",
///     "false": "The customer wants no money.",
/// });
/// const POLICY: Noul = noul!({
///     "question": "Does the reply obey the policy?",
///     "policy": "A reply never promises a date.",
/// });
///
/// # async fn run() -> system2::Result<()> {
/// let product = "the mobile app";
/// let about = noul!("Is the ticket about {product}?");
///
/// let answer = URGENT.ask("The site is down.").await?;
/// if answer.is_likely() { /* … */ }
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct Noul {
    instructions: Json,
    criteria: Criteria,
}

#[derive(Debug, Clone, Serialize)]
struct Criteria {
    #[serde(skip_serializing_if = "Option::is_none")]
    r#true: Option<Json>,
    #[serde(skip_serializing_if = "Option::is_none")]
    r#false: Option<Json>,
}

impl Noul {
    /// Makes a Noul from instructions that come from data.
    ///
    /// The instructions can be any `Serialize` value, as long as it serializes to text, an
    /// object, or an array.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidNoul::Instructions`] if the instructions fail to serialize, are empty, or
    /// are a bare number or `bool`.
    pub fn new(instructions: impl Serialize) -> Result<Self, InvalidNoul> {
        let instructions = json::validate(instructions).map_err(InvalidNoul::Instructions)?;
        let instructions = Json::from(instructions);
        let criteria = Criteria {
            r#true: None,
            r#false: None,
        };
        Ok(Noul {
            instructions,
            criteria,
        })
    }

    /// Makes a Noul from instructions and criteria that come from data.
    ///
    /// The criteria must serialize to an object with a `"true"` key, a `"false"` key, or both.
    /// Each criterion says what that answer means, and follows the same rules as the
    /// instructions.
    ///
    /// # Errors
    ///
    /// - [`InvalidNoul::Instructions`] if the instructions break a rule of [`Noul::new`].
    /// - [`InvalidNoul::Criteria`] if the criteria are not an object, have any other key, or
    ///   are empty.
    /// - [`InvalidNoul::True`] or [`InvalidNoul::False`] if that criterion breaks a rule of the
    ///   instructions.
    pub fn with_criteria(
        instructions: impl Serialize,
        criteria: impl Serialize,
    ) -> Result<Self, InvalidNoul> {
        let instructions = json::validate(instructions).map_err(InvalidNoul::Instructions)?;
        let instructions = Json::from(instructions);
        let criteria = Criteria::new(criteria)?;
        Ok(Noul {
            instructions,
            criteria,
        })
    }

    /// Attaches a tag to the question, so the answer comes back with it.
    ///
    /// This is the same as [`Ask::tag`](crate::Ask::tag), but as a `const fn`, so you can keep a
    /// tagged question in a constant.
    pub const fn tag<T>(self, tag: T) -> Tagged<Self, T> {
        Tagged::new(self, tag)
    }

    /// Builds the answer that a response with this probability of yes would give.
    ///
    /// It's for tests of your own decision code: hand the answer to the code that decides, with
    /// no request. It goes through the same checks as a response from the API.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidAnswer`] if the probability is not from 0 to 1.
    pub fn answer(self, probability: f64) -> Result<NoulAnswer, InvalidAnswer> {
        self.validated_answer(probability)
            .map_err(InvalidAnswer::new)
    }

    fn validated_answer(self, probability: f64) -> Result<NoulAnswer, String> {
        validate_range("probability", probability, 1.0)?;
        let question = self;
        Ok(NoulAnswer {
            question,
            probability,
        })
    }
}

#[doc(hidden)]
#[must_use]
pub const fn noul(instructions: Json, r#true: Option<Json>, r#false: Option<Json>) -> Noul {
    let criteria = Criteria { r#true, r#false };
    Noul {
        instructions,
        criteria,
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
        let r#true = take("true").map_err(InvalidNoul::True)?.map(Json::from);
        let r#false = take("false").map_err(InvalidNoul::False)?.map(Json::from);
        Ok(Criteria { r#true, r#false })
    }

    fn is_empty(&self) -> bool {
        self.r#true.is_none() && self.r#false.is_none()
    }
}

impl QuestionSet for Noul {
    type AnswerSet = NoulAnswer;
}

/// The error from [`Noul::new`] and [`Noul::with_criteria`].
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum InvalidNoul {
    /// The instructions are invalid.
    #[error("the instructions {0}")]
    Instructions(InvalidJson),
    /// The criteria are not an object with a `true` key, a `false` key, or both.
    #[error("the criteria must be an object with the key `true`, the key `false`, or both")]
    Criteria,
    /// The `true` criterion is invalid.
    #[error("the `true` criterion {0}")]
    True(InvalidJson),
    /// The `false` criterion is invalid.
    #[error("the `false` criterion {0}")]
    False(InvalidJson),
}

/// The answer to a [`Noul`]: the probability of yes.
///
/// Serializes as `{"probability": p}`.
#[derive(Debug, Clone)]
pub struct NoulAnswer {
    question: Noul,
    probability: f64,
}

impl NoulAnswer {
    /// Returns the question this answer is for.
    #[must_use]
    pub fn question(&self) -> &Noul {
        &self.question
    }

    /// Returns the question by value, so you can ask it again.
    #[must_use]
    pub fn into_question(self) -> Noul {
        self.question
    }

    /// Returns the probability of yes, from 0 to 1, exactly as the API gives it.
    #[must_use]
    pub fn probability(&self) -> f64 {
        self.probability
    }

    /// Returns `true` if the probability of yes is 0.5 or more.
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
    instructions: &'a Json,
    #[serde(skip_serializing_if = "Option::is_none")]
    criteria: Option<&'a Criteria>,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum Answer {
    Noul { noul: f64 },
}

impl Sealed for Noul {
    fn write(&mut self, raw_question_set: &mut RawQuestionSet) {
        let instructions = &self.instructions;
        let criteria = if self.criteria.is_empty() {
            None
        } else {
            Some(&self.criteria)
        };
        raw_question_set.push(&Question {
            instructions,
            criteria,
        });
    }

    fn read(
        self,
        raw_answer_set: &mut RawAnswerSet,
    ) -> Result<<Self as QuestionSet>::AnswerSet, String> {
        let Answer::Noul { noul: probability } = raw_answer_set.next()?;
        self.validated_answer(probability)
    }
}

#[cfg(test)]
mod tests {
    use super::{InvalidNoul, Noul, NoulAnswer};
    use crate::InvalidJson;
    use crate::ask::wire::{RawAnswerSet, RawQuestionSet, Sealed};
    use serde_json::{Value, json};
    use std::collections::BTreeMap;

    fn serialized(mut noul: Noul) -> Value {
        let mut raw_question_set = RawQuestionSet::default();
        noul.write(&mut raw_question_set);
        serde_json::from_str(raw_question_set.texts()[0]).unwrap()
    }

    fn read(noul: Noul, answer: Value) -> Result<NoulAnswer, String> {
        noul.read(&mut RawAnswerSet::of(vec![answer]))
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
    fn write_gives_the_type_and_the_instructions() {
        let noul = Noul::new("Is it spam?").unwrap();

        assert_eq!(
            serialized(noul),
            json!({ "type": "noul", "instructions": "Is it spam?" })
        );
    }

    #[test]
    fn write_gives_only_the_criteria_of_the_noul() {
        let cases = [
            json!({ "true": "It sells a product." }),
            json!({ "false": { "covers": "questions and replies" } }),
            json!({ "true": "It sells a product.", "false": "It is a reply." }),
        ];
        for criteria in cases {
            let noul = Noul::with_criteria("Is it spam?", &criteria).unwrap();
            assert_eq!(
                serialized(noul),
                json!({ "type": "noul", "instructions": "Is it spam?", "criteria": criteria }),
                "{criteria}"
            );
        }
    }

    #[test]
    fn read_takes_the_probability_and_keeps_the_question() {
        let noul = Noul::new("Is it spam?").unwrap();

        let answer = read(noul, json!({ "type": "noul", "noul": 0.92 })).unwrap();

        assert_eq!(answer.probability(), 0.92);
        assert_eq!(
            serialized(answer.question().clone())["instructions"],
            "Is it spam?"
        );
        assert_eq!(
            serialized(answer.into_question())["instructions"],
            "Is it spam?"
        );
    }

    #[test]
    fn read_refuses_an_answer_that_does_not_fit() {
        let cases = [
            (
                json!({ "type": "box", "confidence": 0.9 }),
                "must answer each question: unknown variant `box`, expected `noul`",
            ),
            (
                json!({ "type": "noul", "noul": -0.01 }),
                "must give a probability from 0 to 1, not -0.01",
            ),
            (
                json!({ "type": "noul", "noul": 1.01 }),
                "must give a probability from 0 to 1, not 1.01",
            ),
        ];
        for (json, message) in cases {
            let noul = Noul::new("Is it spam?").unwrap();
            assert_eq!(read(noul, json).unwrap_err(), message);
        }
    }

    #[test]
    fn serialize_gives_the_probability() {
        let noul = Noul::new("Is it spam?").unwrap();
        let answer = read(noul, json!({ "type": "noul", "noul": 0.92 })).unwrap();

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

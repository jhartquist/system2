use crate::ask::{self, private::Sealed};
use crate::{Ask, InvalidJson, json};
use serde::{Deserialize, Serialize, Serializer};
use serde_json::Value;
use serde_json::value::RawValue;
use std::collections::HashMap;

const MAX_LEVELS: usize = 10;

#[derive(Debug, Clone)]
pub struct Score<L> {
    instructions: Box<RawValue>,
    levels: Vec<Entry<L>>,
}

#[derive(Debug, Clone)]
struct Entry<L> {
    level: Box<RawValue>,
    value: L,
}

#[derive(Serialize)]
#[serde(tag = "type", rename = "score")]
struct Question<'a, L> {
    instructions: &'a RawValue,
    #[serde(serialize_with = "criteria")]
    criteria: &'a [Entry<L>],
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum Answer {
    Score {
        score: f64,
        confidence: f64,
        probabilities: HashMap<String, f64>,
    },
}

impl<L> Score<L> {
    pub fn new(
        instructions: impl Serialize,
        levels: impl IntoIterator<Item = L>,
    ) -> Result<Self, InvalidScore>
    where
        L: Serialize,
    {
        Self::checked(instructions, levels, |level| json::validate(level))
    }

    pub fn with_values<D: Serialize>(
        instructions: impl Serialize,
        values: impl IntoIterator<Item = L>,
        mut level: impl FnMut(&L) -> D,
    ) -> Result<Self, InvalidScore> {
        Self::checked(instructions, values, |value| json::validate(level(value)))
    }

    fn checked(
        instructions: impl Serialize,
        values: impl IntoIterator<Item = L>,
        mut level: impl FnMut(&L) -> Result<Box<RawValue>, InvalidJson>,
    ) -> Result<Self, InvalidScore> {
        let instructions = json::validate(instructions).map_err(InvalidScore::Instructions)?;
        let levels: Vec<_> = values
            .into_iter()
            .enumerate()
            .map(|(index, value)| match level(&value) {
                Ok(level) => Ok(Entry { level, value }),
                Err(error) => Err(InvalidScore::Level { index, error }),
            })
            .collect::<Result<_, _>>()?;
        if !(1..=MAX_LEVELS).contains(&levels.len()) {
            return Err(InvalidScore::Count(levels.len()));
        }
        let mut indexes = HashMap::new();
        for (second, entry) in levels.iter().enumerate() {
            if let Some(first) = indexes.insert(entry.level.get(), second) {
                return Err(InvalidScore::Duplicate { first, second });
            }
        }
        Ok(Score {
            instructions,
            levels,
        })
    }
}

impl<L> Ask for Score<L> {
    type Answer = ScoreAnswer<L>;
}

impl<L> Sealed for Score<L> {
    fn to_json(&self) -> impl Serialize {
        let instructions = &self.instructions;
        let criteria = self.levels.as_slice();
        Question {
            instructions,
            criteria,
        }
    }

    fn into_answer(self, json: Value) -> Result<<Self as Ask>::Answer, String> {
        let Answer::Score {
            score,
            confidence,
            mut probabilities,
        } = ask::from_json(json)?;
        let count = probabilities.len();
        let probabilities = (0..self.levels.len())
            .map(|index| probabilities.remove(&index.to_string()))
            .collect::<Option<Vec<_>>>()
            .filter(|_| count == self.levels.len())
            .ok_or("must give a probability for each level of the Score, and no other")?;

        // of two equal probabilities, the lower level is the more likely
        let most_likely = (0..probabilities.len())
            .max_by(|&a, &b| {
                probabilities[a]
                    .total_cmp(&probabilities[b])
                    .then(b.cmp(&a))
            })
            .unwrap_or_default();
        let question = self;
        Ok(ScoreAnswer {
            question,
            score,
            confidence,
            probabilities,
            most_likely,
        })
    }
}

fn criteria<L, S: Serializer>(levels: &&[Entry<L>], serializer: S) -> Result<S::Ok, S::Error> {
    serializer.collect_seq(levels.iter().map(|l| &l.level))
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum InvalidScore {
    #[error("the instructions {0}")]
    Instructions(InvalidJson),
    #[error("the level at index {index} {error}")]
    Level { index: usize, error: InvalidJson },
    #[error("a Score must have 1 to 10 levels, not {0}")]
    Count(usize),
    #[error(
        "each level must be different, but the levels at index {first} and {second} are the same"
    )]
    Duplicate { first: usize, second: usize },
}

#[derive(Debug, Clone)]
pub struct ScoreAnswer<L> {
    question: Score<L>,
    score: f64,
    confidence: f64,
    probabilities: Vec<f64>,
    most_likely: usize,
}

#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub struct ScoreProbability<'a, L> {
    pub value: &'a L,
    pub probability: f64,
}

impl<L> ScoreAnswer<L> {
    #[must_use]
    pub fn question(&self) -> &Score<L> {
        &self.question
    }

    #[must_use]
    pub fn into_question(self) -> Score<L> {
        self.question
    }

    #[must_use]
    pub fn score(&self) -> f64 {
        self.score
    }

    #[must_use]
    pub fn most_likely(&self) -> &L {
        &self.question.levels[self.most_likely].value
    }

    #[must_use]
    pub fn into_most_likely(self) -> L {
        let mut levels = self.question.levels;
        levels.swap_remove(self.most_likely).value
    }

    #[must_use]
    pub fn confidence(&self) -> f64 {
        self.confidence
    }

    #[must_use]
    pub fn probabilities(&self) -> impl ExactSizeIterator<Item = ScoreProbability<'_, L>> {
        self.question
            .levels
            .iter()
            .zip(&self.probabilities)
            .map(|(level, &probability)| ScoreProbability {
                value: &level.value,
                probability,
            })
    }

    #[must_use]
    pub fn into_probabilities(self) -> Vec<(L, f64)> {
        self.question
            .levels
            .into_iter()
            .zip(self.probabilities)
            .map(|(level, probability)| (level.value, probability))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::{InvalidScore, Score, ScoreAnswer};
    use crate::InvalidJson;
    use crate::ask::private::Sealed;
    use serde::Serialize;
    use serde_json::{Value, json};

    fn serialized<L>(score: &Score<L>) -> String {
        serde_json::to_string(&score.to_json()).unwrap()
    }

    fn answer(probabilities: &Value) -> Value {
        json!({
            "type": "score",
            "score": 1.3,
            "confidence": 0.4,
            "probabilities": probabilities,
            "legend": { "0": "A line.", "1": "A function.", "2": "A module." },
        })
    }

    // one type of level, so coverage sees each check of `checked` and its success together
    fn numbered(count: usize) -> Vec<Value> {
        (0..count).map(|i| json!(format!("Level {i}."))).collect()
    }

    fn sizes(probabilities: &Value) -> ScoreAnswer<&'static str> {
        let score = Score::new("How big?", ["A line.", "A function.", "A module."]).unwrap();
        score.into_answer(answer(probabilities)).unwrap()
    }

    #[test]
    fn new_sends_the_levels_in_order() {
        let score = Score::new("How big?", ["A line.", "A function.", "A module."]).unwrap();

        assert_eq!(
            serialized(&score),
            r#"{"type":"score","instructions":"How big?","criteria":["A line.","A function.","A module."]}"#
        );
    }

    #[test]
    fn with_values_sends_the_level_of_each_value() {
        #[derive(Serialize)]
        struct Span {
            unit: &'static str,
            count: u32,
        }

        let score =
            Score::with_values("How long?", [1, 5], |&count| Span { unit: "day", count }).unwrap();

        assert_eq!(
            serialized(&score),
            r#"{"type":"score","instructions":"How long?","criteria":[{"unit":"day","count":1},{"unit":"day","count":5}]}"#
        );
    }

    #[test]
    fn new_refuses_bad_levels() {
        let cases = [
            (vec![], "a Score must have 1 to 10 levels, not 0"),
            (numbered(11), "a Score must have 1 to 10 levels, not 11"),
            (
                vec![json!("Some."), json!(" ")],
                "the level at index 1 must not be empty",
            ),
            (
                vec![json!("Some."), json!(null)],
                "the level at index 1 must not be empty",
            ),
            (
                vec![json!(1)],
                "the level at index 0 must be text, an object, or an array",
            ),
            (
                vec![json!("Some."), json!("More."), json!("Some.")],
                "each level must be different, but the levels at index 0 and 2 are the same",
            ),
            (
                vec![json!({ "at_least": 1 }), json!({ "at_least": 1 })],
                "each level must be different, but the levels at index 0 and 1 are the same",
            ),
        ];
        for (levels, message) in cases {
            let error = Score::new("How much?", levels).unwrap_err();
            assert_eq!(error.to_string(), message);
        }
    }

    #[test]
    fn new_takes_1_to_10_levels() {
        for count in [1, 10] {
            let score = Score::new("How much?", numbered(count));
            assert!(score.is_ok(), "{count}");
        }
    }

    #[test]
    fn constructors_refuse_bad_instructions() {
        let errors = [
            Score::new("", ["Some."]).unwrap_err(),
            Score::with_values(json!({}), [1], |_| "Some.").unwrap_err(),
        ];
        for error in errors {
            assert!(matches!(
                error,
                InvalidScore::Instructions(InvalidJson::Empty)
            ));
            assert_eq!(error.to_string(), "the instructions must not be empty");
        }
    }

    #[test]
    fn into_answer_keeps_the_values_of_the_api() {
        let answer = sizes(&json!({ "0": 0.1, "1": 0.5, "2": 0.4 }));

        assert_eq!(answer.score(), 1.3);
        assert_eq!(answer.confidence(), 0.4);
        let probabilities: Vec<_> = answer
            .probabilities()
            .map(|level| (*level.value, level.probability))
            .collect();
        assert_eq!(
            probabilities,
            [("A line.", 0.1), ("A function.", 0.5), ("A module.", 0.4)]
        );
        assert_eq!(answer.probabilities().len(), 3);
    }

    #[test]
    fn into_answer_refuses_an_answer_that_does_not_fit() {
        let cases = [
            (
                json!({ "type": "noul", "noul": 0.5 }),
                "must answer each question: unknown variant `noul`, expected `score`",
            ),
            (
                answer(&json!({ "0": 0.5, "1": 0.5 })),
                "must give a probability for each level of the Score, and no other",
            ),
            (
                answer(&json!({ "0": 0.2, "1": 0.4, "3": 0.4 })),
                "must give a probability for each level of the Score, and no other",
            ),
            (
                answer(&json!({ "0": 0.2, "1": 0.4, "2": 0.2, "3": 0.2 })),
                "must give a probability for each level of the Score, and no other",
            ),
        ];
        for (json, message) in cases {
            let score = Score::new("How big?", ["A line.", "A function.", "A module."]).unwrap();
            assert_eq!(score.into_answer(json).unwrap_err(), message);
        }
    }

    #[test]
    fn most_likely_takes_the_highest_probability_then_the_lower_level() {
        let cases = [
            (json!({ "0": 0.0, "1": 0.0, "2": 1.0 }), "A module."),
            (json!({ "0": 0.0, "1": 0.5, "2": 0.5 }), "A function."),
            (json!({ "0": 0.4, "1": 0.2, "2": 0.4 }), "A line."),
        ];
        for (probabilities, level) in cases {
            assert_eq!(
                *sizes(&probabilities).most_likely(),
                level,
                "{probabilities}"
            );
        }
    }

    #[test]
    fn into_methods_give_the_values_back() {
        let probabilities = json!({ "0": 0.1, "1": 0.5, "2": 0.4 });

        assert_eq!(sizes(&probabilities).into_most_likely(), "A function.");
        assert_eq!(
            sizes(&probabilities).into_probabilities(),
            [("A line.", 0.1), ("A function.", 0.5), ("A module.", 0.4)]
        );

        let question = sizes(&probabilities).into_question();
        assert_eq!(
            serialized(sizes(&probabilities).question()),
            serialized(&question)
        );
        assert!(serialized(&question).contains(r#""instructions":"How big?""#));
    }
}

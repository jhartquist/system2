use super::QuestionSet;
use serde::de::DeserializeOwned;
use serde::{Serialize, Serializer};
use serde_json::Value;
use serde_json::value::{RawValue, to_raw_value};
use std::collections::HashMap;

pub trait Sealed {
    fn write(&self, raw_question_set: &mut RawQuestionSet);

    fn read(
        self,
        raw_answer_set: &mut RawAnswerSet,
    ) -> Result<<Self as QuestionSet>::AnswerSet, String>
    where
        Self: QuestionSet;
}

// the questions of a request, in the order of the question set
#[derive(Debug, Default)]
pub struct RawQuestionSet {
    questions: Vec<Box<RawValue>>,
}

impl RawQuestionSet {
    #[expect(clippy::expect_used, reason = "a question holds only checked json")]
    pub(crate) fn push(&mut self, question: &impl Serialize) {
        let question = to_raw_value(question).expect("a question serializes");
        self.questions.push(question);
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.questions.is_empty()
    }

    #[cfg(test)]
    pub(crate) fn texts(&self) -> Vec<&str> {
        self.questions
            .iter()
            .map(|question| question.get())
            .collect()
    }
}

// by hand, as a `serde_json::Map` would sort `q10` before `q2`
impl Serialize for RawQuestionSet {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let keyed = self
            .questions
            .iter()
            .enumerate()
            .map(|(index, question)| (key(index), question));
        serializer.collect_map(keyed)
    }
}

// the answers of a response, read in the order of the questions
#[derive(Debug, Default)]
pub struct RawAnswerSet {
    answers: HashMap<String, Value>,
    next_question_index: usize,
}

impl RawAnswerSet {
    pub(crate) fn new(answers: HashMap<String, Value>) -> Self {
        let next_question_index = 0;
        RawAnswerSet {
            answers,
            next_question_index,
        }
    }

    pub(crate) fn next<T: DeserializeOwned>(&mut self) -> Result<T, String> {
        let key = key(self.next_question_index);
        self.next_question_index += 1;
        let Some(answer) = self.answers.remove(&key) else {
            return Err(format!("must answer each question: `{key}` has no answer"));
        };
        serde_json::from_value(answer)
            .map_err(|error| format!("must answer each question: {error}"))
    }

    #[cfg(test)]
    pub(crate) fn of(answers: Vec<Value>) -> Self {
        let answers = answers
            .into_iter()
            .enumerate()
            .map(|(index, answer)| (key(index), answer))
            .collect();
        RawAnswerSet::new(answers)
    }
}

fn key(index: usize) -> String {
    format!("q{index}")
}

#[cfg(test)]
mod tests {
    use super::{RawAnswerSet, RawQuestionSet, key};
    use serde_json::{Value, json};
    use std::collections::HashMap;

    #[test]
    fn key_names_the_position_in_the_question_set() {
        assert_eq!(key(0), "q0");
        assert_eq!(key(11), "q11");
    }

    #[test]
    fn serialize_keys_the_questions_in_order() {
        let mut raw_question_set = RawQuestionSet::default();
        for index in 0..11 {
            raw_question_set.push(&index);
        }

        // a sorted map puts `q10` before `q2`
        assert_eq!(
            serde_json::to_string(&raw_question_set).unwrap(),
            r#"{"q0":0,"q1":1,"q2":2,"q3":3,"q4":4,"q5":5,"q6":6,"q7":7,"q8":8,"q9":9,"q10":10}"#
        );
    }

    #[test]
    fn next_takes_the_answers_in_order() {
        let answers = HashMap::from([
            ("q1".to_owned(), json!("second")),
            ("q0".to_owned(), json!("first")),
        ]);
        let mut raw_answer_set = RawAnswerSet::new(answers);

        assert_eq!(raw_answer_set.next::<Value>().unwrap(), "first");
        assert_eq!(raw_answer_set.next::<Value>().unwrap(), "second");
        assert_eq!(
            raw_answer_set.next::<Value>().unwrap_err(),
            "must answer each question: `q2` has no answer"
        );
    }

    #[test]
    fn next_refuses_an_answer_of_another_shape() {
        let mut raw_answer_set = RawAnswerSet::of(vec![json!("first")]);

        assert_eq!(
            raw_answer_set.next::<u32>().unwrap_err(),
            r#"must answer each question: invalid type: string "first", expected u32"#
        );
    }
}

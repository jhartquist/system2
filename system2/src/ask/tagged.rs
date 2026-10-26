use super::wire::{RawAnswerSet, RawQuestionSet, Sealed};
use crate::QuestionSet;

#[derive(Debug, Clone)]
pub struct Tagged<Q, T> {
    question_set: Q,
    tag: T,
}

impl<Q, T> Tagged<Q, T> {
    pub(crate) const fn new(question_set: Q, tag: T) -> Self {
        Tagged { question_set, tag }
    }
}

impl<Q: QuestionSet, T> QuestionSet for Tagged<Q, T> {
    type AnswerSet = (Q::AnswerSet, T);
}

impl<Q: QuestionSet, T> Sealed for Tagged<Q, T> {
    fn write(&self, raw_question_set: &mut RawQuestionSet) {
        self.question_set.write(raw_question_set);
    }

    fn read(
        self,
        raw_answer_set: &mut RawAnswerSet,
    ) -> Result<<Self as QuestionSet>::AnswerSet, String> {
        let answer_set = self.question_set.read(raw_answer_set)?;
        Ok((answer_set, self.tag))
    }
}

#[cfg(test)]
mod tests {
    use crate::ask::wire::{RawAnswerSet, RawQuestionSet, Sealed};
    use crate::{Noul, QuestionSet};
    use serde_json::json;

    fn noul(text: &str) -> Noul {
        Noul::new(text).unwrap()
    }

    fn texts(question_set: &impl QuestionSet) -> Vec<String> {
        let mut raw_question_set = RawQuestionSet::default();
        question_set.write(&mut raw_question_set);
        raw_question_set
            .texts()
            .into_iter()
            .map(str::to_owned)
            .collect()
    }

    #[test]
    fn write_gives_the_questions_and_not_the_tag() {
        let question_set = (noul("a"), noul("b"));

        assert_eq!(
            texts(&question_set.clone().tag("tag")),
            texts(&question_set)
        );
    }

    #[test]
    fn read_gives_the_answer_set_and_the_tag() {
        let question_set = (noul("a"), noul("b")).tag("tag");
        let answers = vec![
            json!({ "type": "noul", "noul": 0.1 }),
            json!({ "type": "noul", "noul": 0.2 }),
        ];

        let ((a, b), tag) = question_set.read(&mut RawAnswerSet::of(answers)).unwrap();

        assert_eq!(a.probability(), 0.1);
        assert_eq!(b.probability(), 0.2);
        assert_eq!(tag, "tag");
    }

    #[test]
    fn read_stops_at_a_missing_answer() {
        let question_set = noul("a").tag("tag");

        let error = question_set
            .read(&mut RawAnswerSet::of(vec![]))
            .unwrap_err();

        assert_eq!(error, "must answer each question: `q0` has no answer");
    }
}

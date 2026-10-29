use super::wire::{RawAnswerSet, RawQuestionSet, Sealed};
use crate::QuestionSet;
use std::{any, fmt};

pub struct Questions<A: NamedAnswerSet> {
    question_set: Option<A::QuestionSet>,
}

#[doc(hidden)]
#[diagnostic::on_unimplemented(
    message = "`{Self}` is not a named answer set",
    label = "not a named answer set",
    note = "define the answers with `questions!(Name {{ … }})`, then `Questions<Name>` asks them"
)]
pub trait NamedAnswerSet: Sized {
    type QuestionSet: QuestionSet<AnswerSet = Self::AnswerSet>;
    type AnswerSet;

    fn question_set() -> Self::QuestionSet;

    fn from_answer_set(answer_set: Self::AnswerSet) -> Self;
}

impl<A: NamedAnswerSet> Questions<A> {
    #[must_use]
    pub const fn new() -> Self {
        Questions { question_set: None }
    }
}

impl<A: NamedAnswerSet> QuestionSet for Questions<A> {
    type AnswerSet = A;
}

impl<A: NamedAnswerSet> Sealed for Questions<A> {
    // the set keeps the questions that it sends, so their values go to the answer
    fn write(&mut self, raw_question_set: &mut RawQuestionSet) {
        let question_set = self.question_set.insert(A::question_set());
        question_set.write(raw_question_set);
    }

    fn read(
        self,
        raw_answer_set: &mut RawAnswerSet,
    ) -> Result<<Self as QuestionSet>::AnswerSet, String> {
        let question_set = self.question_set.unwrap_or_else(A::question_set);
        let answer_set = question_set.read(raw_answer_set)?;
        Ok(A::from_answer_set(answer_set))
    }
}

impl<A: NamedAnswerSet> Default for Questions<A> {
    fn default() -> Self {
        Questions::new()
    }
}

// a clone makes its questions again when it is asked, so the questions need no `Clone`
impl<A: NamedAnswerSet> Clone for Questions<A> {
    fn clone(&self) -> Self {
        Questions::new()
    }
}

impl<A: NamedAnswerSet> fmt::Debug for Questions<A> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Questions<{}>", any::type_name::<A>())
    }
}

#[cfg(test)]
mod tests {
    use super::{NamedAnswerSet, Questions};
    use crate::ask::wire::{RawAnswerSet, RawQuestionSet, Sealed};
    use crate::{Noul, NoulAnswer, QuestionSet};
    use serde_json::json;
    use std::any;

    fn noul(text: &str) -> Noul {
        Noul::new(text).unwrap()
    }

    #[derive(Debug)]
    struct Review {
        buggy: NoulAnswer,
        tested: Option<NoulAnswer>,
    }

    impl NamedAnswerSet for Review {
        type QuestionSet = (Noul, Option<Noul>);
        type AnswerSet = (NoulAnswer, Option<NoulAnswer>);

        fn question_set() -> Self::QuestionSet {
            (noul("Is it buggy?"), Some(noul("Is it tested?")))
        }

        fn from_answer_set((buggy, tested): Self::AnswerSet) -> Self {
            Review { buggy, tested }
        }
    }

    fn texts(mut question_set: impl QuestionSet) -> Vec<String> {
        let mut raw_question_set = RawQuestionSet::default();
        question_set.write(&mut raw_question_set);
        raw_question_set
            .texts()
            .into_iter()
            .map(str::to_owned)
            .collect()
    }

    #[test]
    fn write_gives_the_questions_of_the_named_answer_set() {
        assert_eq!(
            texts(Questions::<Review>::new()),
            texts(Review::question_set())
        );
    }

    #[test]
    fn read_gives_the_named_answer_set() {
        let answers = vec![
            json!({ "type": "noul", "noul": 0.1 }),
            json!({ "type": "noul", "noul": 0.2 }),
        ];

        let review = Questions::<Review>::new()
            .read(&mut RawAnswerSet::of(answers))
            .unwrap();

        assert_eq!(review.buggy.probability(), 0.1);
        assert_eq!(review.tested.unwrap().probability(), 0.2);
    }

    #[test]
    fn read_stops_at_a_missing_answer() {
        let answers = vec![json!({ "type": "noul", "noul": 0.1 })];

        let error = Questions::<Review>::new()
            .read(&mut RawAnswerSet::of(answers))
            .unwrap_err();

        assert_eq!(error, "must answer each question: `q1` has no answer");
    }

    #[test]
    fn default_gives_the_questions_of_new() {
        assert_eq!(
            texts(Questions::<Review>::default()),
            texts(Questions::<Review>::new())
        );
    }

    #[test]
    fn debug_names_the_answer_set() {
        assert_eq!(
            format!("{:?}", Questions::<Review>::new()),
            format!("Questions<{}>", any::type_name::<Review>())
        );
    }
}

use super::wire::{RawAnswerSet, RawQuestionSet, Sealed};
use crate::QuestionSet;
use std::marker::PhantomData;
use std::{any, fmt};

pub struct Questions<A: Answers> {
    answers: PhantomData<fn() -> A>,
}

#[diagnostic::on_unimplemented(
    message = "`{Self}` is not the answers of a named set",
    label = "not the answers of a named set",
    note = "add `#[derive(Answers)]` to a struct with named fields, \
            then `Questions<{Self}>` asks its questions"
)]
pub trait Answers: SealedAnswers {}

// hidden, so a named set comes from the derive with constant questions; a caller can still
// reach it through `__private`, but the crate supports no implementation by hand
#[doc(hidden)]
#[diagnostic::on_unimplemented(
    message = "`{Self}` must derive `Answers`, not implement it",
    label = "an implementation of `Answers` by hand",
    note = "add `#[derive(Answers)]` to a struct with named fields, \
            or use `questions! {{ … }}` for other question sets"
)]
pub trait SealedAnswers: Sized {
    type AnswerSet;

    fn question_set() -> impl QuestionSet<AnswerSet = Self::AnswerSet>;

    fn from_answer_set(answer_set: Self::AnswerSet) -> Self;
}

// ties a question of the derive to the answer of its field, so a wrong question is an error at
// the question; the opaque type keeps the error of the whole set to the same short message
#[doc(hidden)]
pub fn question_set<A, Q: QuestionSet<AnswerSet = A>>(
    question_set: Q,
) -> impl QuestionSet<AnswerSet = A> {
    question_set
}

impl<A: Answers> Questions<A> {
    #[must_use]
    pub const fn new() -> Self {
        let answers = PhantomData;
        Questions { answers }
    }
}

impl<A: Answers> QuestionSet for Questions<A> {
    type AnswerSet = A;
}

// the derive cannot name the type of the question set, so the set makes its questions again to
// read the answers; the derive gives constant questions, so they are equal each time
impl<A: Answers> Sealed for Questions<A> {
    fn write(&mut self, raw_question_set: &mut RawQuestionSet) {
        A::question_set().write(raw_question_set);
    }

    fn read(
        self,
        raw_answer_set: &mut RawAnswerSet,
    ) -> Result<<Self as QuestionSet>::AnswerSet, String> {
        let answer_set = A::question_set().read(raw_answer_set)?;
        Ok(A::from_answer_set(answer_set))
    }
}

impl<A: Answers> Default for Questions<A> {
    fn default() -> Self {
        Questions::new()
    }
}

impl<A: Answers> Clone for Questions<A> {
    fn clone(&self) -> Self {
        Questions::new()
    }
}

impl<A: Answers> fmt::Debug for Questions<A> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Questions<{}>", any::type_name::<A>())
    }
}

#[cfg(test)]
mod tests {
    use super::{Answers, Questions, SealedAnswers};
    use crate::ask::wire::{RawAnswerSet, RawQuestionSet, Sealed};
    use crate::{Noul, NoulAnswer, QuestionSet};
    use serde_json::json;
    use std::any;

    fn noul(text: &str) -> Noul {
        Noul::new(text).unwrap()
    }

    // by hand, as the derive writes it
    #[derive(Debug)]
    struct Review {
        buggy: NoulAnswer,
        tested: Option<NoulAnswer>,
    }

    impl Answers for Review {}

    impl SealedAnswers for Review {
        type AnswerSet = (NoulAnswer, Option<NoulAnswer>);

        fn question_set() -> impl QuestionSet<AnswerSet = Self::AnswerSet> {
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
    fn write_gives_the_questions_of_the_answers() {
        assert_eq!(
            texts(Questions::<Review>::new()),
            texts(Review::question_set())
        );
    }

    #[test]
    fn read_gives_the_answers() {
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
    fn default_and_clone_give_the_questions_of_new() {
        let question_set = Questions::<Review>::new();

        assert_eq!(
            texts(question_set.clone()),
            texts(Questions::<Review>::new())
        );
        assert_eq!(texts(Questions::<Review>::default()), texts(question_set));
    }

    #[test]
    fn debug_names_the_answers() {
        assert_eq!(
            format!("{:?}", Questions::<Review>::new()),
            format!("Questions<{}>", any::type_name::<Review>())
        );
    }

    #[test]
    fn question_set_gives_the_question_set_back() {
        assert_eq!(
            texts(super::question_set(noul("Is it buggy?"))),
            ["{\"type\":\"noul\",\"instructions\":\"Is it buggy?\"}"]
        );
    }
}

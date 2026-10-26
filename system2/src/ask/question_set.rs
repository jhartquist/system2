use super::wire::{RawAnswerSet, RawQuestionSet, Sealed};
use crate::{AskFuture, Client};
use serde::Serialize;
use std::array;

pub trait QuestionSet: Sized + Sealed {
    type AnswerSet;

    fn ask<S: Serialize + ?Sized>(self, state: &S) -> AskFuture<Self> {
        Client::global().ask(self, state)
    }
}

pub trait AskIterator: Iterator + Sized {
    fn ask<S: Serialize + ?Sized>(self, state: &S) -> AskFuture<Vec<Self::Item>>
    where
        Self::Item: QuestionSet,
    {
        QuestionSet::ask(self.collect::<Vec<_>>(), state)
    }
}

impl<I: Iterator> AskIterator for I {}

impl<T: QuestionSet> QuestionSet for Vec<T> {
    type AnswerSet = Vec<T::AnswerSet>;
}

impl<T: QuestionSet> Sealed for Vec<T> {
    fn write(&self, raw_question_set: &mut RawQuestionSet) {
        for question_set in self {
            question_set.write(raw_question_set);
        }
    }

    fn read(
        self,
        raw_answer_set: &mut RawAnswerSet,
    ) -> Result<<Self as QuestionSet>::AnswerSet, String> {
        self.into_iter()
            .map(|question_set| question_set.read(raw_answer_set))
            .collect()
    }
}

impl<T: QuestionSet, const N: usize> QuestionSet for [T; N] {
    type AnswerSet = [T::AnswerSet; N];
}

impl<T: QuestionSet, const N: usize> Sealed for [T; N] {
    fn write(&self, raw_question_set: &mut RawQuestionSet) {
        for question_set in self {
            question_set.write(raw_question_set);
        }
    }

    // stable rust has no fallible `array::from_fn`, so the answers go through a `Vec`
    #[expect(clippy::expect_used, reason = "n question sets read n answer sets")]
    fn read(
        self,
        raw_answer_set: &mut RawAnswerSet,
    ) -> Result<<Self as QuestionSet>::AnswerSet, String> {
        let mut answer_sets = Vec::from(self).read(raw_answer_set)?.into_iter();
        Ok(array::from_fn(|_| {
            answer_sets
                .next()
                .expect("one answer set for each question set")
        }))
    }
}

impl<T: QuestionSet> QuestionSet for Option<T> {
    type AnswerSet = Option<T::AnswerSet>;
}

impl<T: QuestionSet> Sealed for Option<T> {
    fn write(&self, raw_question_set: &mut RawQuestionSet) {
        if let Some(question_set) = self {
            question_set.write(raw_question_set);
        }
    }

    fn read(
        self,
        raw_answer_set: &mut RawAnswerSet,
    ) -> Result<<Self as QuestionSet>::AnswerSet, String> {
        match self {
            Some(question_set) => Ok(Some(question_set.read(raw_answer_set)?)),
            None => Ok(None),
        }
    }
}

macro_rules! tuple {
    ($($question_set:ident $index:tt),+) => {
        impl<$($question_set: QuestionSet),+> QuestionSet for ($($question_set,)+) {
            type AnswerSet = ($($question_set::AnswerSet,)+);
        }

        impl<$($question_set: QuestionSet),+> Sealed for ($($question_set,)+) {
            fn write(&self, raw_question_set: &mut RawQuestionSet) {
                $(self.$index.write(raw_question_set);)+
            }

            fn read(
                self,
                raw_answer_set: &mut RawAnswerSet,
            ) -> Result<<Self as QuestionSet>::AnswerSet, String> {
                Ok(($(self.$index.read(raw_answer_set)?,)+))
            }
        }
    };
}

tuple!(A 0);
tuple!(A 0, B 1);
tuple!(A 0, B 1, C 2);
tuple!(A 0, B 1, C 2, D 3);
tuple!(A 0, B 1, C 2, D 3, E 4);
tuple!(A 0, B 1, C 2, D 3, E 4, F 5);
tuple!(A 0, B 1, C 2, D 3, E 4, F 5, G 6);
tuple!(A 0, B 1, C 2, D 3, E 4, F 5, G 6, H 7);
tuple!(A 0, B 1, C 2, D 3, E 4, F 5, G 6, H 7, I 8);
tuple!(A 0, B 1, C 2, D 3, E 4, F 5, G 6, H 7, I 8, J 9);
tuple!(A 0, B 1, C 2, D 3, E 4, F 5, G 6, H 7, I 8, J 9, K 10);
tuple!(A 0, B 1, C 2, D 3, E 4, F 5, G 6, H 7, I 8, J 9, K 10, L 11);

#[cfg(test)]
mod tests {
    use super::QuestionSet;
    use crate::Noul;
    use crate::ask::wire::{RawAnswerSet, RawQuestionSet};
    use serde_json::{Value, json};

    fn noul(text: &str) -> Noul {
        Noul::new(text).unwrap()
    }

    fn instructions(question_set: &impl QuestionSet) -> Vec<Value> {
        let mut raw_question_set = RawQuestionSet::default();
        question_set.write(&mut raw_question_set);
        raw_question_set
            .texts()
            .iter()
            .map(|text| serde_json::from_str::<Value>(text).unwrap()["instructions"].clone())
            .collect()
    }

    fn read<Q: QuestionSet>(
        question_set: Q,
        probabilities: &[f64],
    ) -> Result<Q::AnswerSet, String> {
        let answers = probabilities
            .iter()
            .map(|probability| json!({ "type": "noul", "noul": probability }))
            .collect();
        question_set.read(&mut RawAnswerSet::of(answers))
    }

    #[test]
    fn write_gives_the_questions_of_nested_question_sets_in_order() {
        let question_set = (
            [noul("a"), noul("b")],
            vec![noul("c")],
            (Some(noul("d")), None::<Noul>),
        );

        assert_eq!(instructions(&question_set), ["a", "b", "c", "d"]);
    }

    #[test]
    fn read_gives_each_question_set_its_answers_in_order() {
        let question_set = (
            [noul("a"), noul("b")],
            vec![noul("c")],
            (Some(noul("d")), None::<Noul>),
        );

        let ([a, b], c, (d, none)) = read(question_set, &[0.1, 0.2, 0.3, 0.4]).unwrap();

        assert_eq!(a.probability(), 0.1);
        assert_eq!(b.probability(), 0.2);
        assert_eq!(c[0].probability(), 0.3);
        assert_eq!(d.unwrap().probability(), 0.4);
        assert!(none.is_none());
    }

    #[test]
    fn read_answers_an_empty_question_set_with_no_answers() {
        assert!(read(Vec::<Noul>::new(), &[]).unwrap().is_empty());
        assert!(read(None::<Noul>, &[]).unwrap().is_none());
    }

    #[test]
    fn read_stops_at_a_missing_answer() {
        let question_set = (noul("a"), vec![noul("b")]);

        let error = read(question_set, &[0.5]).unwrap_err();

        assert_eq!(error, "must answer each question: `q1` has no answer");
    }

    #[test]
    fn read_takes_tuples_of_1_to_12_question_sets_in_order() {
        macro_rules! assert_reads_in_order {
            ($($index:tt)+) => {
                let question_set = ($(noul(stringify!($index)),)+);
                let probabilities = [$(f64::from($index) / 100.0),+];
                let answer_set = read(question_set, &probabilities).unwrap();
                assert_eq!([$(answer_set.$index.probability()),+], probabilities);
            };
        }

        assert_reads_in_order!(0);
        assert_reads_in_order!(0 1);
        assert_reads_in_order!(0 1 2);
        assert_reads_in_order!(0 1 2 3);
        assert_reads_in_order!(0 1 2 3 4);
        assert_reads_in_order!(0 1 2 3 4 5);
        assert_reads_in_order!(0 1 2 3 4 5 6);
        assert_reads_in_order!(0 1 2 3 4 5 6 7);
        assert_reads_in_order!(0 1 2 3 4 5 6 7 8);
        assert_reads_in_order!(0 1 2 3 4 5 6 7 8 9);
        assert_reads_in_order!(0 1 2 3 4 5 6 7 8 9 10);
        assert_reads_in_order!(0 1 2 3 4 5 6 7 8 9 10 11);
    }
}

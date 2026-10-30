use super::wire::{RawAnswerSet, RawQuestionSet, Sealed};
use crate::QuestionSet;
use std::marker::PhantomData;

// a field of `questions! { … }`: a question set, which the request asks, or another value,
// which is not sent and moves into the answers as it is
#[doc(hidden)]
pub trait Field {
    type AnswerSet;

    fn write(&mut self, raw_question_set: &mut RawQuestionSet);

    fn read(self, raw_answer_set: &mut RawAnswerSet) -> Result<Self::AnswerSet, String>;
}

impl<Q: QuestionSet> Field for Q {
    type AnswerSet = Q::AnswerSet;

    fn write(&mut self, raw_question_set: &mut RawQuestionSet) {
        Sealed::write(self, raw_question_set);
    }

    fn read(self, raw_answer_set: &mut RawAnswerSet) -> Result<Self::AnswerSet, String> {
        Sealed::read(self, raw_answer_set)
    }
}

#[doc(hidden)]
#[derive(Debug, Clone)]
pub struct Value<T>(T);

impl<T> Field for Value<T> {
    type AnswerSet = T;

    fn write(&mut self, _raw_question_set: &mut RawQuestionSet) {}

    fn read(self, _raw_answer_set: &mut RawAnswerSet) -> Result<Self::AnswerSet, String> {
        Ok(self.0)
    }
}

// a call, so the closure sees the type of the value before it selects the kind of the field
#[doc(hidden)]
pub fn field<T, F>(value: T, into_field: impl FnOnce(T) -> F) -> F {
    into_field(value)
}

// the type of a field selects its kind by autoref: a method on more references wins, so
// `(&&&&&&Probe::of(&field)).field_kind()` tries each kind from the top, and a value is last
#[doc(hidden)]
pub struct Probe<T>(PhantomData<T>);

impl<T> Probe<T> {
    pub fn of(_field: &T) -> Self {
        Probe(PhantomData)
    }
}

#[doc(hidden)]
pub trait QuestionSetKind {
    fn field_kind(&self) -> QuestionSetField;
}

impl<Q: QuestionSet> QuestionSetKind for &&&&&Probe<Q> {
    fn field_kind(&self) -> QuestionSetField {
        QuestionSetField
    }
}

#[doc(hidden)]
pub trait ReferenceKind {
    fn field_kind(&self) -> ReferenceField;
}

impl<B: BorrowedQuestionSet> ReferenceKind for &&&&Probe<B> {
    fn field_kind(&self) -> ReferenceField {
        ReferenceField
    }
}

// a level lower, as `&B` with `B = &Q` would overlap the impl above
impl<B: BorrowedQuestionSet> ReferenceKind for &&&Probe<&B> {
    fn field_kind(&self) -> ReferenceField {
        ReferenceField
    }
}

impl<B: BorrowedQuestionSet> ReferenceKind for &&&Probe<&mut B> {
    fn field_kind(&self) -> ReferenceField {
        ReferenceField
    }
}

#[doc(hidden)]
pub trait BorrowedQuestionSet {}

impl<Q: QuestionSet> BorrowedQuestionSet for &Q {}

impl<Q: QuestionSet> BorrowedQuestionSet for &mut Q {}

#[doc(hidden)]
pub trait ResultKind {
    fn field_kind(&self) -> ResultField;
}

impl<Q: QuestionSet, E> ResultKind for &&Probe<Result<Q, E>> {
    fn field_kind(&self) -> ResultField {
        ResultField
    }
}

#[doc(hidden)]
pub trait IteratorKind {
    fn field_kind(&self) -> IteratorField;
}

impl<I> IteratorKind for &Probe<I>
where
    I: IntoIterator,
    I::Item: QuestionSetOrResult,
{
    fn field_kind(&self) -> IteratorField {
        IteratorField
    }
}

#[doc(hidden)]
pub trait QuestionSetOrResult {}

impl<Q: QuestionSet> QuestionSetOrResult for Q {}

impl<Q: QuestionSet, E> QuestionSetOrResult for Result<Q, E> {}

#[doc(hidden)]
pub trait ValueKind {
    fn field_kind(&self) -> ValueField;
}

impl<T> ValueKind for Probe<T> {
    fn field_kind(&self) -> ValueField {
        ValueField
    }
}

#[doc(hidden)]
pub struct QuestionSetField;

#[expect(clippy::unused_self, reason = "the kind is the type of `self`")]
impl QuestionSetField {
    pub fn into_field<Q: QuestionSet>(self, field: Q) -> Q {
        field
    }
}

#[doc(hidden)]
pub struct ValueField;

#[expect(clippy::unused_self, reason = "the kind is the type of `self`")]
impl ValueField {
    pub fn into_field<T>(self, field: T) -> Value<T> {
        Value(field)
    }
}

// a field of a wrong kind fails the bound of its `into_field`, at the field, with a fix: the
// traits of these bounds have no impls, so the bound always fails

#[doc(hidden)]
pub struct ReferenceField;

#[expect(clippy::unused_self, reason = "the kind is the type of `self`")]
impl ReferenceField {
    pub fn into_field<T: OwnedQuestionSet>(self, field: T) -> Value<T> {
        Value(field)
    }
}

#[doc(hidden)]
#[diagnostic::on_unimplemented(
    message = "a field of `questions!` must own its question set, not borrow it",
    label = "a reference to a question set",
    note = "to ask a question again, clone it"
)]
pub trait OwnedQuestionSet {}

#[doc(hidden)]
pub struct ResultField;

#[expect(clippy::unused_self, reason = "the kind is the type of `self`")]
impl ResultField {
    pub fn into_field<T: QuestionSetNotResult>(self, field: T) -> Value<T> {
        Value(field)
    }
}

#[doc(hidden)]
#[diagnostic::on_unimplemented(
    message = "a field of `questions!` must be a question set, not a `Result` of one",
    label = "a `Result` of a question set",
    note = "add `?` to give the question set, or the error to the caller"
)]
pub trait QuestionSetNotResult {}

#[doc(hidden)]
pub struct IteratorField;

#[expect(clippy::unused_self, reason = "the kind is the type of `self`")]
impl IteratorField {
    pub fn into_field<T: QuestionSetNotIterator>(self, field: T) -> Value<T> {
        Value(field)
    }
}

#[doc(hidden)]
#[diagnostic::on_unimplemented(
    message = "a field of `questions!` must be a question set, not an iterator of them",
    label = "an iterator of question sets",
    note = "collect it into a `Vec`, which is a question set: `.collect::<Vec<_>>()`, \
            or `.collect::<Result<Vec<_>, _>>()?` for `Result`s"
)]
pub trait QuestionSetNotIterator {}

#[cfg(test)]
mod tests {
    use super::{
        Field, IteratorField, IteratorKind, OwnedQuestionSet, Probe, QuestionSetField,
        QuestionSetKind, QuestionSetNotIterator, QuestionSetNotResult, ReferenceField,
        ReferenceKind, ResultField, ResultKind, Value, ValueField, ValueKind,
    };
    use crate::ask::wire::{RawAnswerSet, RawQuestionSet};
    use crate::{Noul, QuestionError};
    use serde_json::json;

    fn noul(text: &str) -> Noul {
        Noul::new(text).unwrap()
    }

    fn texts(mut field: impl Field) -> Vec<String> {
        let mut raw_question_set = RawQuestionSet::default();
        field.write(&mut raw_question_set);
        raw_question_set
            .texts()
            .into_iter()
            .map(str::to_owned)
            .collect()
    }

    #[test]
    fn write_gives_the_questions_of_a_question_set_and_none_of_a_value() {
        assert_eq!(
            texts((noul("a"), noul("b"))),
            [
                r#"{"type":"noul","instructions":"a"}"#,
                r#"{"type":"noul","instructions":"b"}"#
            ]
        );
        assert_eq!(texts(ValueField.into_field(7)), Vec::<String>::new());
    }

    #[test]
    fn read_gives_the_answer_set_of_a_question_set() {
        let answers = vec![json!({ "type": "noul", "noul": 0.1 })];

        let answer = Field::read(noul("a"), &mut RawAnswerSet::of(answers)).unwrap();

        assert_eq!(answer.probability(), 0.1);
    }

    #[test]
    fn read_gives_a_value_back_and_reads_no_answer() {
        let mut raw_answer_set = RawAnswerSet::of(vec![json!({ "type": "noul", "noul": 0.1 })]);

        let value = ValueField
            .into_field("tag")
            .read(&mut raw_answer_set)
            .unwrap();

        assert_eq!(value, "tag");
        assert!(noul("a").read(&mut raw_answer_set).is_ok());
    }

    #[test]
    #[expect(
        clippy::needless_borrow,
        reason = "`questions!` writes each probe this way"
    )]
    fn field_kind_selects_each_kind_by_type() {
        let mut question_set = noul("a");
        let option = Some(noul("a"));
        let mut borrowed = &noul("a");
        let result = Ok::<_, QuestionError>(noul("a"));
        let iterator = [noul("a")].into_iter();
        let results = [Ok::<_, QuestionError>(noul("a"))].into_iter();
        let value = &Some(7);

        let _: QuestionSetField = (&&&&&&Probe::of(&option)).field_kind();
        let _: ReferenceField = (&&&&&&Probe::of(&borrowed)).field_kind();
        let _: ReferenceField = (&&&&&&Probe::of(&&borrowed)).field_kind();
        let _: ReferenceField = (&&&&&&Probe::of(&&mut question_set)).field_kind();
        let _: ReferenceField = (&&&&&&Probe::of(&&&mut question_set)).field_kind();
        let _: ReferenceField = (&&&&&&Probe::of(&&mut borrowed)).field_kind();
        let _: ResultField = (&&&&&&Probe::of(&result)).field_kind();
        let _: IteratorField = (&&&&&&Probe::of(&iterator)).field_kind();
        let _: IteratorField = (&&&&&&Probe::of(&results)).field_kind();
        let _: ValueField = (&&&&&&Probe::of(&value)).field_kind();
        let _: ValueField = (&&&&&&Probe::of(&&value)).field_kind();
        let _: QuestionSetField = (&&&&&&Probe::of(&question_set)).field_kind();
    }

    // only to run `into_field`: a caller never gets here, as rustc refuses each wrong kind
    struct Refused;

    impl OwnedQuestionSet for Refused {}
    impl QuestionSetNotResult for Refused {}
    impl QuestionSetNotIterator for Refused {}

    #[test]
    fn into_field_gives_a_value_for_each_wrong_kind() {
        let _: Value<Refused> = ReferenceField.into_field(Refused);
        let _: Value<Refused> = ResultField.into_field(Refused);
        let _: Value<Refused> = IteratorField.into_field(Refused);
        let _: Noul = QuestionSetField.into_field(noul("a"));
    }
}

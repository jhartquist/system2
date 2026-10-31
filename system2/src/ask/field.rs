use crate::{Noul, QuestionSet};
use std::marker::PhantomData;

// a call, so the closure sees the type of the value before it selects the kind of the field
#[doc(hidden)]
pub fn field<T, F>(value: T, into_field: impl FnOnce(T) -> F) -> F {
    into_field(value)
}

// the type of a field selects its kind by autoref: a method on more references wins, so
// `(&&&&&&Probe::of(&field)).field_kind()` tries each kind from the top, and other data is last
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
pub trait DataKind {
    fn field_kind(&self) -> DataField;
}

impl<T> DataKind for Probe<T> {
    fn field_kind(&self) -> DataField {
        DataField
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

// these unimplemented bounds report errors at the offending field, and the empty question set
// avoids cascading errors at `ask`
type EmptyQuestionSet = [Noul; 0];

#[doc(hidden)]
pub struct ReferenceField;

#[expect(clippy::unused_self, reason = "the kind is the type of `self`")]
impl ReferenceField {
    pub fn into_field<T: OwnedQuestionSet>(self, _field: T) -> EmptyQuestionSet {
        []
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
    pub fn into_field<T: QuestionSetNotResult>(self, _field: T) -> EmptyQuestionSet {
        []
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
    pub fn into_field<T: QuestionSetNotIterator>(self, _field: T) -> EmptyQuestionSet {
        []
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

// the real `QuestionSet` bound, so rustc suggests it for unconstrained type parameters
#[doc(hidden)]
pub struct DataField;

#[expect(clippy::unused_self, reason = "the kind is the type of `self`")]
impl DataField {
    pub fn into_field<T: QuestionSet>(self, _field: T) -> EmptyQuestionSet {
        []
    }
}

#[cfg(test)]
mod tests {
    use super::{
        DataField, DataKind, EmptyQuestionSet, IteratorField, IteratorKind, OwnedQuestionSet,
        Probe, QuestionSetField, QuestionSetKind, QuestionSetNotIterator, QuestionSetNotResult,
        ReferenceField, ReferenceKind, ResultField, ResultKind,
    };
    use crate::{Noul, QuestionError};

    fn noul(text: &str) -> Noul {
        Noul::new(text).unwrap()
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
        let data = &Some(7);
        let borrowed_three_times = &&&option;
        let borrowed_list = vec![&option];
        let borrowed_iterator = [&option].into_iter();

        let _: QuestionSetField = (&&&&&&Probe::of(&option)).field_kind();
        let _: ReferenceField = (&&&&&&Probe::of(&borrowed)).field_kind();
        let _: ReferenceField = (&&&&&&Probe::of(&&borrowed)).field_kind();
        let _: ReferenceField = (&&&&&&Probe::of(&&mut question_set)).field_kind();
        let _: ReferenceField = (&&&&&&Probe::of(&&&mut question_set)).field_kind();
        let _: ReferenceField = (&&&&&&Probe::of(&&mut borrowed)).field_kind();
        let _: ResultField = (&&&&&&Probe::of(&result)).field_kind();
        let _: IteratorField = (&&&&&&Probe::of(&iterator)).field_kind();
        let _: IteratorField = (&&&&&&Probe::of(&results)).field_kind();
        let _: DataField = (&&&&&&Probe::of(&data)).field_kind();
        let _: DataField = (&&&&&&Probe::of(&&data)).field_kind();
        let _: DataField = (&&&&&&Probe::of(&borrowed_three_times)).field_kind();
        let _: DataField = (&&&&&&Probe::of(&borrowed_list)).field_kind();
        let _: DataField = (&&&&&&Probe::of(&borrowed_iterator)).field_kind();
        let _: QuestionSetField = (&&&&&&Probe::of(&question_set)).field_kind();
    }

    // only to run `into_field`: a caller never gets here, as rustc refuses each wrong kind
    struct Refused;

    impl OwnedQuestionSet for Refused {}
    impl QuestionSetNotResult for Refused {}
    impl QuestionSetNotIterator for Refused {}

    #[test]
    fn into_field_gives_an_empty_question_set_for_each_wrong_kind() {
        let _: EmptyQuestionSet = ReferenceField.into_field(Refused);
        let _: EmptyQuestionSet = ResultField.into_field(Refused);
        let _: EmptyQuestionSet = IteratorField.into_field(Refused);
        let _: EmptyQuestionSet = DataField.into_field(noul("a"));
        let _: Noul = QuestionSetField.into_field(noul("a"));
    }
}

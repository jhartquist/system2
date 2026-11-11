use crate::{InvalidBbox, InvalidChoice, InvalidNoul, InvalidScore};

/// An answer that `answer` refused, because a value breaks a rule of its question.
///
/// The message tells the rule, as in "the answer must give a probability from 0 to 1".
#[derive(Debug, thiserror::Error)]
#[error("the answer {message}")]
pub struct InvalidAnswer {
    message: String,
}

impl InvalidAnswer {
    pub(crate) fn new(message: impl Into<String>) -> Self {
        let message = message.into();
        InvalidAnswer { message }
    }
}

/// The error of each question constructor.
///
/// It lets one `Result` hold questions of different types, as in a `questions!` set from data.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum QuestionError {
    /// A constructor of [`Noul`](crate::Noul) failed.
    #[error(transparent)]
    Noul(#[from] InvalidNoul),
    /// A constructor of [`Choice`](crate::Choice) failed.
    #[error(transparent)]
    Choice(#[from] InvalidChoice),
    /// A constructor of [`Score`](crate::Score) failed.
    #[error(transparent)]
    Score(#[from] InvalidScore),
    /// A constructor of [`Bbox`](crate::Bbox) failed.
    #[error(transparent)]
    Bbox(#[from] InvalidBbox),
}

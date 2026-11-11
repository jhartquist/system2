use crate::{
    AskError, CacheError, EnvError, InvalidAnswer, InvalidBbox, InvalidChoice, InvalidImage,
    InvalidNoul, InvalidScore, QuestionError,
};

/// Each error of the crate.
///
/// Each error type converts into it, so `?` works in a function that returns [`Result`].
/// [`InvalidJson`](crate::InvalidJson) is only a part of another error, so it does not convert.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// A constructor refused a question.
    #[error(transparent)]
    Question(#[from] QuestionError),
    /// An answer that a test built breaks a rule of its question.
    #[error(transparent)]
    Answer(#[from] InvalidAnswer),
    /// An image breaks a rule of [`Image`](crate::Image).
    #[error(transparent)]
    Image(#[from] InvalidImage),
    /// A request failed.
    #[error(transparent)]
    Ask(#[from] AskError),
    /// The client cannot read or write the cache file.
    #[error(transparent)]
    Cache(#[from] CacheError),
    /// A variable of the environment has a bad value.
    #[error(transparent)]
    Env(#[from] EnvError),
}

impl From<InvalidNoul> for Error {
    fn from(error: InvalidNoul) -> Self {
        Error::Question(QuestionError::Noul(error))
    }
}

impl From<InvalidChoice> for Error {
    fn from(error: InvalidChoice) -> Self {
        Error::Question(QuestionError::Choice(error))
    }
}

impl From<InvalidScore> for Error {
    fn from(error: InvalidScore) -> Self {
        Error::Question(QuestionError::Score(error))
    }
}

impl From<InvalidBbox> for Error {
    fn from(error: InvalidBbox) -> Self {
        Error::Question(QuestionError::Bbox(error))
    }
}

/// A `Result` with [`Error`] as its default error.
pub type Result<T, E = Error> = std::result::Result<T, E>;

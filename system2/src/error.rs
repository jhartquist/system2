use crate::{AskError, InvalidChoice, InvalidNoul, InvalidScore, QuestionError};

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    #[error(transparent)]
    Question(#[from] QuestionError),
    #[error(transparent)]
    Ask(#[from] AskError),
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

pub type Result<T, E = Error> = std::result::Result<T, E>;

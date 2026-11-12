use crate::{InvalidBbox, InvalidChoice, InvalidNoul, InvalidScore};

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

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum QuestionError {
    #[error(transparent)]
    Noul(#[from] InvalidNoul),
    #[error(transparent)]
    Choice(#[from] InvalidChoice),
    #[error(transparent)]
    Score(#[from] InvalidScore),
    #[error(transparent)]
    Bbox(#[from] InvalidBbox),
}

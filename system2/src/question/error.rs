use crate::{InvalidChoice, InvalidNoul, InvalidScore};

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum QuestionError {
    #[error(transparent)]
    Noul(#[from] InvalidNoul),
    #[error(transparent)]
    Choice(#[from] InvalidChoice),
    #[error(transparent)]
    Score(#[from] InvalidScore),
}

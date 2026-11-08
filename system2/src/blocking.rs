mod ask;
mod client;

pub use ask::{Ask, AskIterator};
pub use client::Client;

pub mod prelude {
    #[doc(no_inline)]
    pub use super::{Ask, AskIterator};
    #[doc(no_inline)]
    pub use crate::{
        Answers, Levels, Options, QuestionSet, Questions, bbox, choice, image, noul, questions,
        score,
    };
}

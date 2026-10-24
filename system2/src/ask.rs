mod asking;
mod error;

use crate::Client;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

pub use asking::Asking;
pub use error::AskError;

pub trait Ask: Sized + private::Sealed {
    type Answer;

    fn ask<S: Serialize + ?Sized>(self, state: &S) -> Asking<Self> {
        Client::global().ask(self, state)
    }
}

pub(crate) mod private {
    use super::Ask;
    use serde::Serialize;
    use serde_json::Value;

    pub trait Sealed {
        fn to_json(&self) -> impl Serialize;

        fn into_answer(self, json: Value) -> Result<<Self as Ask>::Answer, String>
        where
            Self: Ask;
    }
}

pub(crate) fn from_json<T: DeserializeOwned>(json: Value) -> Result<T, String> {
    serde_json::from_value(json).map_err(|error| format!("must answer each question: {error}"))
}

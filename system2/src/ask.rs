use crate::client::RawAnswer;
use crate::{AskError, Client};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::fmt;
use std::pin::Pin;
use std::task::{Context, Poll, ready};

type Request = Pin<Box<dyn Future<Output = Result<RawAnswer, AskError>> + Send>>;

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

#[must_use = "a request is sent only when its answers are awaited"]
pub struct Asking<A> {
    // outside the request, so the future is `Send` only when the question set is
    question_set: Option<A>,
    request: Request,
}

impl<A> Asking<A> {
    pub(crate) fn new(
        question_set: A,
        request: impl Future<Output = Result<RawAnswer, AskError>> + Send + 'static,
    ) -> Self {
        let question_set = Some(question_set);
        let request = Box::pin(request);
        Asking {
            question_set,
            request,
        }
    }
}

// the question set is never pinned, only moved out
impl<A> Unpin for Asking<A> {}

impl<A: Ask> Future for Asking<A> {
    type Output = Result<A::Answer, AskError>;

    #[expect(
        clippy::expect_used,
        reason = "a future is not polled after it completes"
    )]
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let RawAnswer { json, request_id } = ready!(self.request.as_mut().poll(cx))?;
        let question_set = self
            .question_set
            .take()
            .expect("the question set answers once");
        let answers = question_set
            .into_answer(json)
            .map_err(|message| AskError::InvalidResponse {
                message,
                request_id,
            });
        Poll::Ready(answers)
    }
}

impl<A> fmt::Debug for Asking<A> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Asking").finish_non_exhaustive()
    }
}

pub(crate) fn from_json<T: DeserializeOwned>(json: Value) -> Result<T, String> {
    serde_json::from_value(json).map_err(|error| format!("must answer each question: {error}"))
}

#[cfg(test)]
mod tests {
    use crate::{Client, Noul};

    #[test]
    fn debug_shows_only_the_name() {
        let asking = Client::new().ask(Noul::new("Is it spam?").unwrap(), "Buy now!");

        assert_eq!(format!("{asking:?}"), "Asking { .. }");
    }
}

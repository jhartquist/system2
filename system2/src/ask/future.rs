use crate::client::Response;
use crate::{AskError, QuestionSet};
use std::fmt;
use std::pin::Pin;
use std::task::{Context, Poll, ready};

type Request = Pin<Box<dyn Future<Output = Result<Response, AskError>> + Send>>;

#[must_use = "a request is sent only when its answers are awaited"]
pub struct AskFuture<Q> {
    // outside the request, so the future is `Send` only when the question set is
    question_set: Option<Q>,
    request: Request,
}

impl<Q> AskFuture<Q> {
    pub(crate) fn new(
        question_set: Q,
        request: impl Future<Output = Result<Response, AskError>> + Send + 'static,
    ) -> Self {
        let question_set = Some(question_set);
        let request = Box::pin(request);
        AskFuture {
            question_set,
            request,
        }
    }
}

// the question set is never pinned, only moved out
impl<Q> Unpin for AskFuture<Q> {}

impl<Q: QuestionSet> Future for AskFuture<Q> {
    type Output = Result<Q::AnswerSet, AskError>;

    #[expect(
        clippy::expect_used,
        reason = "a future is not polled after it completes"
    )]
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let Response {
            raw_answer_set,
            request_id,
        } = ready!(self.request.as_mut().poll(cx))?;
        let question_set = self
            .question_set
            .take()
            .expect("the question set answers once");
        let answer_set = raw_answer_set
            .into_answer_set(question_set)
            .map_err(|message| AskError::InvalidResponse {
                message,
                request_id,
            });
        Poll::Ready(answer_set)
    }
}

impl<Q> fmt::Debug for AskFuture<Q> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AskFuture").finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use crate::{Client, Noul};

    #[test]
    fn debug_shows_only_the_name() {
        let future = Client::new().ask(Noul::new("Is it spam?").unwrap(), "Buy now!");

        assert_eq!(format!("{future:?}"), "AskFuture { .. }");
    }
}

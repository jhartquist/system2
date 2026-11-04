use crate::client::Response;
use crate::{AskError, QuestionSet, Receipt};
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

impl<Q: QuestionSet> AskFuture<Q> {
    #[expect(
        clippy::expect_used,
        reason = "a future is not polled after it completes"
    )]
    fn poll_with_receipt(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(Q::AnswerSet, Receipt), AskError>> {
        let Response {
            raw_answer_set,
            receipt,
            pending_line,
        } = ready!(self.request.as_mut().poll(cx))?;
        let question_set = self
            .question_set
            .take()
            .expect("the question set answers once");
        let answer_set = raw_answer_set
            .into_answer_set(question_set)
            .map_err(|message| AskError::InvalidResponse {
                message,
                request_id: receipt.request_id.clone(),
            })?;
        if let Some(pending_line) = pending_line {
            pending_line.write().map_err(AskError::Cache)?;
        }
        Poll::Ready(Ok((answer_set, receipt)))
    }
}

impl<Q: QuestionSet> Future for AskFuture<Q> {
    type Output = Result<Q::AnswerSet, AskError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.poll_with_receipt(cx)
            .map_ok(|(answer_set, _receipt)| answer_set)
    }
}

#[must_use = "a request is sent only when its answers are awaited"]
pub struct AskWithReceiptFuture<Q> {
    ask_future: AskFuture<Q>,
}

impl<Q> AskWithReceiptFuture<Q> {
    pub(crate) fn new(ask_future: AskFuture<Q>) -> Self {
        AskWithReceiptFuture { ask_future }
    }
}

impl<Q: QuestionSet> Future for AskWithReceiptFuture<Q> {
    type Output = Result<(Q::AnswerSet, Receipt), AskError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.ask_future.poll_with_receipt(cx)
    }
}

impl<Q> fmt::Debug for AskFuture<Q> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AskFuture").finish_non_exhaustive()
    }
}

impl<Q> fmt::Debug for AskWithReceiptFuture<Q> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AskWithReceiptFuture")
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use crate::{Client, Noul};

    #[test]
    fn debug_shows_only_the_name() {
        let client = Client::new();
        let ask_future = client.ask(Noul::new("Is it spam?").unwrap(), "Buy now!");
        let ask_with_receipt_future =
            client.ask_with_receipt(Noul::new("Is it spam?").unwrap(), "Buy now!");

        assert_eq!(format!("{ask_future:?}"), "AskFuture { .. }");
        assert_eq!(
            format!("{ask_with_receipt_future:?}"),
            "AskWithReceiptFuture { .. }"
        );
    }
}

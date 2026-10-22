use crate::{AskError, Noul, NoulAnswer};
use std::fmt;
use std::marker::PhantomData;
use std::pin::Pin;
use std::task::{Context, Poll};

type Request = Pin<Box<dyn Future<Output = Result<NoulAnswer, AskError>> + Send>>;

#[must_use = "a request is sent only when its answers are awaited"]
pub struct Asking<Q> {
    // the future is `Send` only when the set is
    set: PhantomData<Q>,
    request: Request,
}

impl<Q> Asking<Q> {
    pub(crate) fn new(
        request: impl Future<Output = Result<NoulAnswer, AskError>> + Send + 'static,
    ) -> Self {
        let set = PhantomData;
        let request = Box::pin(request);
        Asking { set, request }
    }
}

impl Future for Asking<Noul> {
    type Output = Result<NoulAnswer, AskError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.request.as_mut().poll(cx)
    }
}

impl<Q> fmt::Debug for Asking<Q> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Asking").finish_non_exhaustive()
    }
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

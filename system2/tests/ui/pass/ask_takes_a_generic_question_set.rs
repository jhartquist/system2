#![allow(dead_code, reason = "each function only needs to compile")]

// one import is enough, with `Ask` as the bound
mod async_caller {
    use system2::{Ask, AskError, Client, Receipt};

    async fn ask_tagged<Q: Ask>(question_set: Q) -> Result<(Q::AnswerSet, u32), AskError> {
        question_set.tag(1).ask("Buy now!").await
    }

    async fn ask_with_receipt<Q: Ask>(question_set: Q) -> Result<(Q::AnswerSet, Receipt), AskError> {
        question_set.ask_with_receipt("Buy now!").await
    }

    async fn ask_with<Q: Ask>(
        client: &Client,
        question_set: Q,
    ) -> Result<<Q as system2::QuestionSet>::AnswerSet, AskError> {
        client.ask(question_set, "Buy now!").await
    }
}

// the blocking prelude gives one `ask` to a set with a `QuestionSet` bound
mod blocking_caller {
    use system2::blocking::prelude::*;
    use system2::{AskError, Receipt, blocking};

    fn ask_tagged<Q: QuestionSet>(question_set: Q) -> Result<(Q::AnswerSet, u32), AskError> {
        question_set.tag(1).ask("Buy now!")
    }

    fn ask_with_receipt<Q: QuestionSet>(
        question_set: Q,
    ) -> Result<(Q::AnswerSet, Receipt), AskError> {
        question_set.ask_with_receipt("Buy now!")
    }

    fn ask_with<Q: QuestionSet>(
        client: &blocking::Client,
        question_set: Q,
    ) -> Result<<Q as QuestionSet>::AnswerSet, AskError> {
        client.ask(question_set, "Buy now!")
    }
}

fn main() {}

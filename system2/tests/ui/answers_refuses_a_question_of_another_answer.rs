use system2::{Answers, NoulAnswer, choice};

#[derive(Answers)]
struct Review {
    #[system2(question = choice!("Which language is the code in?", ["rust", "go"]))]
    language: NoulAnswer,
}

fn main() {}

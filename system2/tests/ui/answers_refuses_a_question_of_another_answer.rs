use system2::prelude::*;

#[derive(Answers)]
struct Review {
    #[system2(question = choice!("Which language is the code in?", ["rust", "go"]))]
    language: NoulAnswer,
}

fn main() {}

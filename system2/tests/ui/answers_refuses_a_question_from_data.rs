use system2::{Answers, NoulAnswer};

#[derive(Answers)]
struct Review {
    #[system2(question = system2::Noul::new(RULE)?)]
    rule: NoulAnswer,
}

const RULE: &str = "No unwrap.";

fn main() {}

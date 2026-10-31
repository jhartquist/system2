use system2::{Answers, Noul, NoulAnswer, noul};

const BUGGY: Noul = noul!("Is there a bug in this code?");

#[derive(Answers)]
#[system2(question = BUGGY)]
struct Review {
    #[system2(question = BUGGY)]
    buggy: NoulAnswer,
}

fn main() {}

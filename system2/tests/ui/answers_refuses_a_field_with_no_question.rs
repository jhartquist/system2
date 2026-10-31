use system2::{Answers, Noul, NoulAnswer, noul};

const BUGGY: Noul = noul!("Is there a bug in this code?");

#[derive(Answers)]
struct Review {
    #[system2(question = BUGGY)]
    buggy: NoulAnswer,
    tested: NoulAnswer,
}

fn main() {}

use system2::{Answers, Noul, NoulAnswer, noul};

const BUGGY: Noul = noul!("Is there a bug in this code?");
const TESTED: Noul = noul!("Is the function tested?");

#[derive(Answers)]
struct Review {
    #[system2(question = BUGGY, question = TESTED)]
    buggy: NoulAnswer,
}

fn main() {}

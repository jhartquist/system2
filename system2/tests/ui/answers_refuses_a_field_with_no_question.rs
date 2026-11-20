use system2::prelude::*;

const BUGGY: Noul = noul!("Is there a bug in this code?");

#[derive(Answers)]
struct Review {
    #[system2(question = BUGGY)]
    buggy: NoulAnswer,
    tested: NoulAnswer,
}

fn main() {}

use system2::{Answers, Noul, noul};

const BUGGY: Noul = noul!("Is there a bug in this code?");

#[derive(Answers)]
struct Review<A> {
    #[system2(question = BUGGY)]
    buggy: A,
}

fn main() {}

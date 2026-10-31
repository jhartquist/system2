use system2::{Answers, Noul, NoulAnswer};

#[derive(Answers)]
struct Review {
    #[system2(question = Noul::new("Is there a bug in this code?").unwrap())]
    buggy: NoulAnswer,
}

fn main() {}

use system2::{Answers, NoulAnswer};

#[derive(Answers)]
struct Review {
    #[system2(rename = "bug")]
    buggy: NoulAnswer,
}

fn main() {}

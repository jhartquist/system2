use system2::Noul;
use system2::prelude::*;

fn main() {
    let questions = vec![Noul::new("Is it spam?").unwrap()];
    let _ = questions.iter().ask("Buy now!");
}

use system2::Noul;
use system2::prelude::*;

const BUGGY: Noul = noul!("Is there a bug in this code?");

fn generic<T>(question_set: T) {
    let _ = questions! { question_set };
}

fn main() {
    let nouls = [BUGGY];
    let _ = questions! {
        id: 7,
        count: Some(7),
        deep: &&&BUGGY,
        list: vec![&BUGGY],
        iterator: nouls.iter(),
    }
    .ask("Buy now!");
}

use system2::prelude::*;

fn main() {
    let count = 3;
    let _ = choice!("How many?", [("three", count)]);
}

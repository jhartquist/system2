use system2::prelude::*;

const BUGGY: Noul = noul!("Is there a bug in this code?");

fn main() {
    let _ = questions! { buggy: &BUGGY };
}

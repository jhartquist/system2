use system2::prelude::*;
use system2::Noul;

const BUGGY: Noul = noul!("Is there a bug in this code?");

fn main() {
    let buggy = &BUGGY;
    let _ = questions! { buggy: &buggy };
}

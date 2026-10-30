use system2::prelude::*;
use system2::Noul;

const BUGGY: Noul = noul!("Is there a bug in this code?");

fn main() {
    let _ = questions! { buggy: BUGGY, buggy: BUGGY };
}

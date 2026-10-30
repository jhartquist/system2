use system2::prelude::*;
use system2::Noul;

fn main() {
    let mut buggy: Noul = noul!("Is there a bug in this code?");
    let _ = questions! { buggy: &mut buggy };
}

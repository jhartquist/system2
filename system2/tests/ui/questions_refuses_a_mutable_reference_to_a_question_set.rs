use system2::prelude::*;

fn main() {
    let mut buggy: Noul = noul!("Is there a bug in this code?");
    let _ = questions! { buggy: &mut buggy };
}

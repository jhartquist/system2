use system2::prelude::*;
use system2::Noul;

fn main() {
    let rules = ["No unwrap.", "No panics."];
    let _ = questions! { checks: rules.iter().map(Noul::new) };
}

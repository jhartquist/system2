use system2::prelude::*;

fn main() {
    let rules = ["No unwrap.", "No panics."];
    let _ = questions! { checks: rules.iter().map(Noul::new) };
}

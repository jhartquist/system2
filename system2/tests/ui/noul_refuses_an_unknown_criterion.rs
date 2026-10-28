use system2::prelude::*;

fn main() {
    let _ = noul!("Is it spam?", { "maybe": "It links out." });
    let _ = noul!("Is it spam?", { true: "It sells a product." });
}

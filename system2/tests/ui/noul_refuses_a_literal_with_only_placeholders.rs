use system2::prelude::*;

fn main() {
    let text = "Is it spam?";
    let _ = noul!(" {text} ");
    let _ = noul!("{text:}>0}");
}

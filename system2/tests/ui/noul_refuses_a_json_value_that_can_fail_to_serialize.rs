use system2::prelude::*;

#[derive(serde::Serialize)]
struct Change {
    diff: String,
}

fn main() {
    let change = Change { diff: String::from("+ fn main() {}") };
    let _ = noul!({ "question": "Is the change safe?", "change": change });
}

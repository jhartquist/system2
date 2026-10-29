use system2::prelude::*;

fn main() {
    let rule = "Each function has a test.";
    let _ = choice!("Which check?", [("test", { "rule": rule })]);
}

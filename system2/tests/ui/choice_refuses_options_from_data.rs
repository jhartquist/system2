use system2::prelude::*;

fn main() {
    let teams = vec!["core", "web"];
    let _ = choice!("Which team owns this ticket?", teams);
}

use system2::prelude::*;

fn main() {
    let texts = ["a bug", "a typo"];
    let _ = questions! { checks: texts.iter().map(|text| noul!("Is it {text}?")) };
}

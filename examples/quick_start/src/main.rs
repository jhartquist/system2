//! Asks if a function has a bug, and reads the probability of yes.

use system2::prelude::*;

const CODE: &str = "\
fn is_even(n: u32) -> bool {
    n % 2 == 1
}";

#[tokio::main]
async fn main() -> system2::Result<()> {
    let answer = noul!("Is there a bug in this code?").ask(CODE).await?;

    let probability = answer.probability();
    println!("probability of a bug: {probability:.2}");

    if answer.is_likely() {
        println!("the code needs a fix");
    }

    Ok(())
}

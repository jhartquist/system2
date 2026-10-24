use system2::Options;

#[derive(Options)]
enum Problem {
    #[doc = concat!("The code ", "panics.")]
    Crash,
}

fn main() {}

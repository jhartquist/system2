use system2::Options;

#[derive(Options)]
enum Problem {
    #[system2(rename = "panic", rename = "crash")]
    Crash,
}

fn main() {}

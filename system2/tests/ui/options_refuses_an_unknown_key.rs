use system2::Options;

#[derive(Options)]
enum Problem {
    #[system2(name = "crash")]
    Crash,
}

fn main() {}

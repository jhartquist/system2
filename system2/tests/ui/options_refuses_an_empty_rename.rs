use system2::Options;

#[derive(Options)]
enum Problem {
    #[system2(rename = "")]
    Crash,
}

fn main() {}

use system2::Options;

#[derive(Options)]
#[system2(rename = "problem")]
enum Problem {
    Crash,
}

fn main() {}

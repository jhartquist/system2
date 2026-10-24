use system2::Options;

#[derive(Options)]
enum Problem {
    #[system2(description = json!({ "what": "The code panics." }))]
    Crash,
}

fn main() {}

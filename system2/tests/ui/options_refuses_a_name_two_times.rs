use system2::Options;

#[derive(Options)]
enum Problem {
    HandOff,
    #[system2(rename = "hand_off")]
    Escalate,
}

fn main() {}

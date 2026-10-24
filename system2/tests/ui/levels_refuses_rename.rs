use system2::Levels;

#[derive(Levels)]
enum Severity {
    #[system2(rename = "minor")]
    Minor,
}

fn main() {}

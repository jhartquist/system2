use system2::Levels;

#[derive(Levels)]
enum Severity {
    /// Nothing breaks.
    #[system2(description = "")]
    Minor,
    /// Data is lost.
    Critical,
}

fn main() {}

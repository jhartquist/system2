pub const MAX_LEVELS: usize = 10;

#[must_use]
pub fn count_message(count: usize) -> String {
    format!("a Score must have 1 to {MAX_LEVELS} levels, not {count}")
}

#[must_use]
pub fn duplicate_message(first: usize, second: usize) -> String {
    format!(
        "each level must be different, but the levels at index {first} and {second} are the same"
    )
}

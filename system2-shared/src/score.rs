//! The limits and the messages of the levels of a Score.

/// The most levels that a Score can have.
pub const MAX_LEVELS: usize = 10;

/// Returns the message for a Score with no levels, or with too many.
#[must_use]
pub fn count_message(count: usize) -> String {
    format!("a Score must have 1 to {MAX_LEVELS} levels, not {count}")
}

/// Returns the message for two levels that are the same.
#[must_use]
pub fn duplicate_message(first: usize, second: usize) -> String {
    format!(
        "each level must be different, but the levels at index {first} and {second} are the same"
    )
}

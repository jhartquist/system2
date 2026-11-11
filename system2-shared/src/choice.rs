//! The limits and the messages of the options of a Choice.

/// The most options that a Choice can have.
pub const MAX_OPTIONS: usize = 255;

/// Returns the message for a Choice with no options, or with too many.
#[must_use]
pub fn count_message(count: usize) -> String {
    format!("a Choice must have 1 to {MAX_OPTIONS} options, not {count}")
}

/// Returns the message for two options with the same name.
#[must_use]
pub fn duplicate_message(name: &str) -> String {
    format!("each option must have a different name, but two options have the name `{name}`")
}

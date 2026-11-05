pub const MAX_OPTIONS: usize = 255;

#[must_use]
pub fn count_message(count: usize) -> String {
    format!("a Choice must have 1 to {MAX_OPTIONS} options, not {count}")
}

#[must_use]
pub fn duplicate_message(name: &str) -> String {
    format!("each option must have a different name, but two options have the name `{name}`")
}

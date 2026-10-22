#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum InvalidJson {
    #[error("must serialize to JSON: {0}")]
    Serialize(serde_json::Error),
    #[error("must be text, an object, or an array")]
    Shape,
    #[error("must not be empty")]
    Empty,
}

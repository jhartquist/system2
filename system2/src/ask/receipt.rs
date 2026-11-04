use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Receipt {
    pub model: Option<String>,
    pub usage: Usage,
    pub cached: bool,
    pub latency: Duration,
    pub request_id: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Usage {
    pub input_tokens: u64,
    pub output_tokens: u64,
}

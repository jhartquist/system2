use std::time::Duration;

/// The facts of one request: the model, the tokens, the cache, the time, and the request ID.
///
/// `ask_with_receipt` returns it next to the answers. An answer from the cache has the model of
/// the stored response, zero tokens, zero latency, and no request ID.
///
/// # Examples
///
/// ```no_run
/// use system2::prelude::*;
/// use system2::Usage;
///
/// # async fn run() -> system2::Result<()> {
/// let urgent = noul!("Is the ticket urgent?");
/// let (answer, receipt) = urgent.ask_with_receipt("The site is down.").await?;
/// let Usage { input_tokens, output_tokens, .. } = receipt.usage;
/// println!("{input_tokens} in, {output_tokens} out, {:?}", receipt.latency);
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Receipt {
    /// The version of the model that answered. A set with no questions has no model.
    pub model: Option<String>,
    /// The tokens of the request.
    pub usage: Usage,
    /// `true` if the answer came from the cache.
    pub cached: bool,
    /// The time from the first send to the last response, with the retries and their waits.
    ///
    /// A wait for the limit of [`Client::max_concurrent_requests`] is not in it.
    ///
    /// [`Client::max_concurrent_requests`]: crate::Client::max_concurrent_requests
    pub latency: Duration,
    /// The ID of the request, for TypeSafe support.
    pub request_id: Option<String>,
}

/// The tokens of a request, or the total of a client.
///
/// [`Client::usage`](crate::Client::usage) gives the total of a client and its clones.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Usage {
    /// The tokens of the request.
    pub input_tokens: u64,
    /// The tokens of the response.
    pub output_tokens: u64,
}

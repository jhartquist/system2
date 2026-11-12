# system2

[![crates.io](https://img.shields.io/crates/v/system2.svg)](https://crates.io/crates/system2) [![docs.rs](https://img.shields.io/docsrs/system2)](https://docs.rs/system2) [![CI](https://github.com/jhartquist/system2/actions/workflows/ci.yml/badge.svg)](https://github.com/jhartquist/system2/actions/workflows/ci.yml)

An (unofficial) idiomatic Rust SDK for [TypeSafe](https://typesafe.ai/)'s System One model,
[Jev](https://typesafe.ai/blog/introducing-system-one-models-and-jev).

Daniel Kahneman's *Thinking, Fast and Slow* popularized two modes of thinking:

- **System 1** makes quick, intuitive judgments.
- **System 2** reasons slowly and deliberately.

> the goal is to make System 1 tasks extremely reliable and get System 2 by composing with code
>
> \- [Diogo Almeida](https://x.com/CompleteSkeptic/status/2099981459541143995), CEO of TypeSafe

TypeSafe calls Jev a [System One](https://docs.typesafe.ai/concepts/system-one) model: it answers
narrow questions about an input, called the `state`. Unlike an LLM asked for structured output, Jev
doesn't generate text or produce any chain-of-thought: it answers with calibrated probabilities.
`system2` brings those judgments into Rust. A question or a state can be as simple as a string, or
it can be built from your own enums, structs, and images. Either way, an answer returns the values
you passed in, with their probabilities, so you can compose them into decisions with code.

## Why system2

- **Flexible**: strings, JSON, images, your own enums, or any `Serialize` or `Display` type.
- **Ergonomic**: macros and derives for static questions, options, and levels, with helpful
  compiler errors.
- **Verifiable**: static questions checked at compile time, dynamic ones on creation, and every
  answer on arrival.
- **Composable**: nest tuples, arrays, `Vec`s, and structs of questions in a single request.
- **Testable**: cache and replay recorded answers, and unit test your decisions with no network.
- **Observable**: OpenTelemetry-compatible `tracing` spans with latency, tokens, model, retries,
  and request ID.
- **Resilient**: retries with backoff, timeouts, and a concurrency limit built in.

## Install

[Sign up](https://console.typesafe.ai/home) for TypeSafe, create an
[API key](https://console.typesafe.ai/keys), then:

```sh
cargo add system2
cargo add tokio --features macros,rt-multi-thread
export TYPESAFE_API_KEY=...
```

`system2` is young, so expect breaking changes before 1.0.

## Quick start

Jev has four kinds of question: a [`noul`][Noul] asks yes or no, a [`choice`][Choice] picks one
of your options, a [`score`][Score] picks a level on your ordered scale, and a [`bbox`][Bbox] finds
a region of an image. This example asks two nouls in one request, then decides based on the
probability of yes:

```rust,no_run
use system2::prelude::*;

#[tokio::main]
async fn main() -> system2::Result<()> {
    let ticket = "I was charged twice. Fix it today.";

    let (angry, refund) = (
        noul!("Is the customer angry?"),
        noul!("Does the customer ask for a refund?"),
    )
        .ask(ticket)
        .await?;

    if refund.probability() > 0.9 {
        println!("open a refund");
    } else if angry.probability() > 0.8 {
        println!("send to a person");
    } else {
        println!("add to the queue");
    }
    Ok(())
}
```

Jev answered 0.67 for the refund, below its 0.9 threshold, and 0.85 for angry, above its 0.8
threshold, so this prints `send to a person`.

[`.ask`][Ask::ask] takes any question or set of questions, and a state, and returns the answers in
the shape of the questions. It uses the global client: unless you set your own, the first `.ask`
makes a default one from the environment, such as `TYPESAFE_API_KEY`. The Client section below
shows how to customize it. `.ask` consumes what it asks, so clone a question to ask it again.

The prelude has the macros and the traits. The snippets below are fragments, so `ticket` is whatever
state each one needs. Import each type they use by name, as in `use system2::Noul;`.

## Questions and state

A question and a state can each be any `serde::Serialize` value. Each must serialize to text, an
object, or an array, and an empty one fails before any request is sent. Here the question
carries a struct of your own, and the state is JSON. Add serde with
`cargo add serde --features derive` and `cargo add serde_json`:

```rust,ignore
use serde::Serialize;
use serde_json::json;
use system2::Noul;

#[derive(Serialize)]
struct Policy {
    name: String,
    text: String,
}

let policy = Policy {
    name: "passwords".to_string(),
    text: "A ticket never contains a password.".to_string(),
};
let question = json!({ "question": "Does the ticket break this policy?", "policy": policy });
let ticket = json!({ "plan": "pro", "message": "I was charged twice. Fix it today." });

let broken = Noul::new(question)?.ask(&ticket).await?;
```

A noul can also say what a yes and a no mean. These are its criteria, and you can give one or
both:

```rust
use system2::Noul;
use system2::prelude::*;

const REFUND: Noul = noul!("Does the customer ask for a refund?", {
    "true": "The customer asks for money back.",
    "false": "The customer asks for no money back.",
});
```

A [`Choice`][Choice] asks Jev to pick one of your options, and its answer gives each option a
probability. An option can be any value with `Display`, which gives the option its name.

```rust,ignore
let teams: Vec<String> = load_teams(); // ["billing", "engineering", "support"]

let team = Choice::new("Which team should handle this ticket?", teams)?
    .ask(&ticket)
    .await?;

println!("{}", team.most_likely()); // billing
```

It also takes your own types. The enum is the list of options, its doc comments describe them to
Jev, and the answer is one of your variants. Each option's name is its variant in snake case, such
as `billing`:

```rust,ignore
#[derive(Serialize)]
struct Ticket { id: u64, plan: &'static str, message: &'static str }

#[derive(Debug, Clone, Copy, Options)]
enum Team {
    /// Payments, refunds, and invoices.
    Billing,
    /// Bugs and outages.
    Engineering,
    /// Anything else.
    Support,
}

let team = choice!("Which team should handle this ticket?", Team).ask(&ticket).await?;

println!("{:?}", team.most_likely()); // Billing
```

To keep your doc comments for your own docs, give Jev its text with an attribute instead.
`description` replaces the doc comment, and `rename` changes the name Jev sees:

```rust
use system2::prelude::*;

#[derive(Debug, Clone, Copy, Options)]
enum Team {
    #[system2(rename = "payments", description = "Payments, refunds, and invoices.")]
    Billing,
    #[system2(description = "Bugs and outages.")]
    Engineering,
    #[system2(description = "Anything else.")]
    Support,
}
```

Options can also be written inline: as names, as names with descriptions, or as tuples with a
value of your own that the answer returns:

```rust
use system2::Choice;
use system2::prelude::*;

const LANGUAGE: Choice<&str> = choice!("In which language is the ticket?", ["en", "es"]);

const NEXT: Choice<&str> = choice!("What should support do next?", {
    "reply": "Send an answer to the customer.",
    "escalate": "Send the ticket to a lead.",
});

const DAYS: Choice<u32> = choice!(
    "When should support answer?",
    [
        ("today", "Before the end of the day.", 1),
        ("this_week", "Before the end of the week.", 7),
    ]
);
```

For values from data, [`Choice::with_values`][Choice::with_values] takes your values and a closure
that gives each one its name, or its name and a description. The answer returns the value Jev
picked, with no lookup:

```rust,ignore
#[derive(Deserialize)]
struct Person {
    name: String,
    skills: String,
}

let people: Vec<Person> = load_people();
let owner = Choice::with_values("Who should answer this ticket?", people, |person| {
    (person.name.clone(), person.skills.clone())
})?
.ask(&ticket)
.await?;

let person: Person = owner.into_most_likely();
```

An [`Image`][Image] goes anywhere text does: in a `{placeholder}` of a question, in `json!`, in a
struct, or as an option. Here a customer sends a screenshot, and the code compares it to a picture
of a known bug that ships with the app:

```rust,ignore
const KNOWN_BUG: Image = image!("known_bug.png");

let screenshot = Image::new(bytes)?; // the bytes of the customer's upload

let same = noul!("Does {screenshot} show the same error as {KNOWN_BUG}?")
    .ask("The app shows this error when I pay.")
    .await?;

if same.is_likely() {
    let dialog = bbox!("The error dialog.").ask(&screenshot).await?;
    println!("the dialog is at {:?}", dialog.to_pixels(&screenshot));
}
```

`image!` embeds the file with `include_bytes!` and checks the image when the crate builds, so a
missing or bad image is a compile error. `Image::new` checks bytes that arrive at run time.
`is_likely` is true at a probability of 0.5 or more. A [`bbox`][Bbox] asks where something is,
and `to_pixels` turns its answer into a box in the pixels of the image. Ask a bbox about one image
at a time, because its answer does not say which image holds the box.

## Validation

Each question type has two ways in. A constructor, such as [`Noul::new`][Noul::new] or
[`Noul::with_criteria`][Noul::with_criteria], checks its input at run time and returns a
`Result<Noul, InvalidNoul>`. A macro, such as [`noul!`][noul!], is a shortcut for the same rules,
checked by the compiler, so it gives you a `Noul` with nothing to unwrap. It can still take
run-time values in its placeholders, as in `noul!("Is the ticket about {product}?")`, because the
text around them already proves the question valid. Anything the compiler cannot see, such as a
whole question or the options of a Choice from data, goes through a constructor.

A mistake in a macro is a compile error at the bad token:

```text
error: each option must have a different name, but two options have the name `go`
 --> src/main.rs:4:55
  |
4 |     let _ = choice!("Which language?", ["go", "rust", "go"]);
  |                                                       ^^^^
```

A constructor gives the same message at run time, before any request is sent:

```rust
use system2::Choice;

let teams = vec!["billing", "support", "billing"]; // from a bad config file

if let Err(error) = Choice::new("Which team should handle this ticket?", teams) {
    println!("{error}");
    // each option must have a different name, but two options have the name `billing`
}
```

The answer is checked too. Jev is built to answer inside your options, and the SDK still checks
each response against its question at the boundary, so a bug or a proxy in between gives you an
error instead of a bad value. That is a check on shape, not on truth: a valid answer can still be
a wrong judgment, which is why every answer carries its probabilities.

## Question sets

A set is a group of questions that one request asks. There are three ways to build one:

- A tuple, an array, or a `Vec` of questions. The answers come back in the same shape.
- [`questions!`][questions!], which names each answer, inline.
- `#[derive(Answers)]`, which names them in a struct of your own, to return from a function.

The first example was a tuple of two questions. Sets nest, so a tuple can hold a `Vec` with one
question for each item in your data:

```rust,ignore
let products = load_products(); // ["the mobile app", "the web app"]

let (angry, about) = (
    noul!("Is the customer angry?"),
    products
        .iter()
        .map(|product| noul!("Is the ticket about {product}?"))
        .collect::<Vec<_>>(),
)
    .ask(&ticket)
    .await?;
```

The answers come back in the shape of the questions: `angry` is one answer, and `about` is a
`Vec` with an answer for each product. An array works the same way and gives an array back. An
`Option` of a set asks it only when it is `Some`, as in `paid.then_some(REFUND)`. An iterator of
questions has `.ask` too, and an empty set sends no request at all.

When a set grows, give its answers names with `questions!`. This one asks three things about a
ticket in one request, then decides. The score's levels come from an enum, lowest first. Jev
knows a level only by its text, so each doc comment is the text of its level, and an enum with
no doc comments uses the names of its variants, such as `low`:

```rust,ignore
#[derive(Debug, Levels)]
enum Urgency { Low, Medium, High }

let triage = questions! {
    angry: noul!("Is the customer angry?"),
    team: choice!("Which team should handle this ticket?", Team),
    urgency: score!("How urgent is this ticket?", Urgency),
}
.ask(&ticket)
.await?;

let (&team, team_probability) = triage.team.top(1).next().expect("a Choice has an option");
let urgent = triage.urgency.score() >= 1.5;
let angry = triage.angry.probability() > 0.8;

// act only when Jev gives the team it picked at least 0.9
let decision = if team_probability < 0.9 {
    "ask a person".to_string()
} else if urgent || angry {
    format!("page the {team:?} lead")
} else {
    format!("add to the {team:?} queue")
};
```

`top(1)` gives the option Jev picked, with its probability. A score's `score()` is the mean
level, from 0 to the index of the last level, so here it runs from 0 (low) to 2 (high), and
`most_likely()` is the level with the highest probability. Levels can be inline too, as text or
with your own values, as in `score!("How long is the fix?", [("A day.", 1), ("A week.", 5)])`.

To return the answers from a function, put them in a struct of your own. `#[derive(Answers)]`
takes a question for each field, and [`Questions::new()`][Questions] asks them all, knowing which
from the return type:

```rust,ignore
#[derive(Answers)]
struct Triage {
    #[system2(question = noul!("Is the customer angry?"))]
    angry: NoulAnswer,
    #[system2(question = choice!("Which team should handle this ticket?", Team))]
    team: ChoiceAnswer<Team>,
    #[system2(question = score!("How urgent is this ticket?", Urgency))]
    urgency: ScoreAnswer<Urgency>,
}

async fn triage(ticket: &Ticket) -> system2::Result<Triage> {
    Ok(Questions::new().ask(ticket).await?)
}
```

Five tickets, one request each:

| Ticket | Message | Decision |
| --- | --- | --- |
| 1 | We were charged twice this month. Please fix it today. | page the Billing lead |
| 2 | How do I change my avatar? | add to the Support queue |
| 3 | The export page has shown a 500 error since this morning. Our whole team is blocked. | page the Engineering lead |
| 4 | hello?? | add to the Support queue |
| 5 | The invoice page crashes when I try to pay. | ask a person |

The full program is [`examples/triage`][triage], with the recording behind these numbers. Ticket 5
could be billing or engineering. Jev gave engineering 0.80 and billing 0.20, below the 0.90
threshold, so it goes to a person.

One answer can also pick the next question:

```rust,ignore
if let Team::Billing = team {
    let refund = noul!("Does the customer ask for a refund?").ask(&ticket).await?;
    // ...
}
```

## Client

A [`Client`][Client] sends the requests to the API. To change its settings, such as the model or
the timeout, build one of your own:

```rust,no_run
use std::time::Duration;

use system2::Client;
use system2::prelude::*;

#[tokio::main]
async fn main() -> system2::Result<()> {
    let ticket = "I was charged twice. Fix it today.";

    let client = Client::from_env()?
        .model("jev-1.14.0")
        .timeout(Duration::from_secs(30))
        .max_retries(4)
        .max_concurrent_requests(16);

    let refund = client
        .ask(noul!("Does the customer ask for a refund?"), ticket)
        .await?;
    println!("refund: {:.2}", refund.probability());

    client.set_global();

    let angry = noul!("Is the customer angry?").ask(ticket).await?;
    println!("angry: {:.2}", angry.probability());
    Ok(())
}
```

`client.ask(question, state)` uses only that client, and leaves the global client as it is.
[`set_global`][Client::set_global] makes it the client that every `.ask` uses, so the rest of your
code does not change. A clone of a client shares its connections, its cache, its request limit,
and its token count.

The example pins the model. TypeSafe moves `jev-latest`, the default, to each new version of Jev,
and a pinned model such as `jev-1.14.0` changes only when your code does.

Five settings can come from the environment or from code:

| Setting | Variable | Method | Default |
| --- | --- | --- | --- |
| API key | `TYPESAFE_API_KEY` | `.api_key(key)` | none |
| Base URL | `TYPESAFE_BASE_URL` | `.base_url(url)` | `https://api.typesafe.ai` |
| Model | `TYPESAFE_DEFAULT_MODEL` | `.model(name)` | `jev-latest` |
| Cache file | `SYSTEM2_CACHE` | `.cache(path)?` | none |
| Offline | `SYSTEM2_OFFLINE=1` | `.offline(true)` | off |

The rest, such as `timeout`, `max_retries`, and `long_context`, are set only in code.
[`Client::from_env`][Client::from_env] reads the variables once, when it makes the client, and a
method after it wins: `Client::from_env()?.model("jev-1.14.0")` asks `jev-1.14.0` whatever
`TYPESAFE_DEFAULT_MODEL` says. [`Client::new`][Client::new] reads no environment, so you give
everything in code. With no key, a client answers only from its cache.

## OpenRouter

[OpenRouter](https://openrouter.ai/) also serves Jev, with the same API. Point the client at it
with the two variables from OpenRouter's guide, and the rest of your code stays the same:

```sh
export TYPESAFE_BASE_URL=https://openrouter.ai/api
export TYPESAFE_API_KEY=sk-or-...   # your OpenRouter key
```

Or set both in code, with the key from OpenRouter's own variable:

```rust,ignore
let key = std::env::var("OPENROUTER_API_KEY").expect("set OPENROUTER_API_KEY");
let client = Client::new()
    .base_url("https://openrouter.ai/api")
    .api_key(key)
    .set_global();
```

A few things differ on OpenRouter:

- A pinned model has no patch number: `jev-1.14` on OpenRouter, and `jev-1.14.0` on TypeSafe.
  `jev-latest` works on both.
- Long context is always on, so `long_context(true)` changes nothing.
- The request ID in an error or a receipt is OpenRouter's generation ID, and the span's
  `gen_ai.provider.name` is `openrouter`.

## Testing

Record the answers once, then replay them with no API key and no network:

```sh
SYSTEM2_CACHE=triage.jsonl cargo run                     # asks Jev, and records
SYSTEM2_CACHE=triage.jsonl SYSTEM2_OFFLINE=1 cargo run   # replays, sends nothing
```

In `examples/triage`, change `TEAM_THRESHOLD` from 0.9 to 0.8 and replay: ticket 5 pages the
engineering lead. Change it back, and it goes to a person. Neither replay sends a request. Offline,
a request that changed since the recording (the question, the state, or the model) is an error, not
a quiet call to the API. A replayed answer counts no tokens and has no request ID. The file holds
each response and a hash of its request, not the request itself. Treat it like the rest of your test
data.

The same file makes a test repeatable. Write the test as usual:

```rust,ignore
#[tokio::test]
async fn a_double_charge_goes_to_billing() -> system2::Result<()> {
    let team = choice!("Which team should handle this ticket?", Team)
        .ask("I was charged twice.")
        .await?;
    assert!(matches!(team.most_likely(), Team::Billing));
    Ok(())
}
```

Record it once with your key, commit the file, and run it offline in CI:

```sh
SYSTEM2_CACHE=tests/jev.jsonl cargo test
SYSTEM2_CACHE=tests/jev.jsonl SYSTEM2_OFFLINE=1 cargo test
```

On a client, [`.cache`][Client::cache] and [`.offline`][Client::offline] set the same in code.
Two live runs of one request can give different probabilities: in
the recordings behind this page, the probability that ticket 1 is urgent was 0.82 in one run and
0.83 in the next. The cache is what makes runs the same. A request matches only when its body
matches byte for byte, so use a `BTreeMap`, not a `HashMap`, in a state: a `HashMap` writes its
keys in a different order each run.

### Choosing thresholds

The thresholds on this page are illustrative. Choose yours from labeled data. TypeSafe trains
Jev to be [calibrated](https://docs.typesafe.ai/concepts/system-one): across many answers, those
given 0.8 should be correct about 80% of the time. Calibration describes the aggregate, not any
single answer.

To pick a threshold, record answers for a set of labeled tickets, and replay them at each
candidate value. Choose the value that minimizes the cost of your errors, weighing false positives
against false negatives.

Then lock in the behavior at that threshold with a unit test. A decision is a function of its
answers, and `answer` constructs a real answer from the values you give it, with no API key, no
network, and no runtime:

```rust,ignore
const REFUND: Noul = noul!("Does the customer ask for a refund?");

fn refund_now(refund: &NoulAnswer) -> bool {
    refund.probability() > 0.9
}

#[test]
fn a_refund_needs_more_than_0_9() {
    assert!(!refund_now(&REFUND.answer(0.90).unwrap()));
    assert!(refund_now(&REFUND.answer(0.91).unwrap()));
}
```

`answer` runs the same checks as a response from Jev, so a test cannot build an answer that Jev
could never give: `REFUND.answer(1.2)` is an error. Each question type has an `answer`.

## Concurrency and retries

Ask about a whole queue at once. By default the client keeps at most 8 requests in flight, gives
each attempt 10 s, and retries a 408, a 429, a 5xx, a timeout, or a failed connection twice, with
backoff.

```rust,ignore
use tokio::task::JoinSet;

let client = Client::from_env()?.max_concurrent_requests(2).set_global();

let mut asking = JoinSet::new();
for ticket in &tickets { // tickets: Vec<Ticket>
    asking.spawn(noul!("Is the ticket urgent?").tag(ticket.id).ask(ticket));
}
for answered in asking.join_all().await {
    let (urgent, id) = answered?;
    println!("ticket {id}: urgent {:.2}", urgent.probability());
}

let usage = client.usage();
println!("{} tokens in, {} out", usage.input_tokens, usage.output_tokens);
```

```text
ticket 2: urgent 0.08
ticket 4: urgent 0.20
ticket 3: urgent 0.90
ticket 5: urgent 0.71
ticket 1: urgent 0.83
1507 tokens in, 105 out
```

The five requests took 73 to 184 ms each. The answers arrive in the order they finish, so `.tag`
carries each ticket's ID back next to its answer. The tag is not sent to Jev. `.ask` serializes
the state before it returns, so a spawned task does not borrow the ticket.

## Tracing

Each request is a `tracing` span, so it reaches whatever subscriber you install. Its fields have
the model that answered, the tokens, the latency, and the request ID, and the `gen_ai.*` names
follow OpenTelemetry's conventions for generative AI. Each attempt is a `DEBUG` event in the span,
or a `WARN` event with the error and the wait when a retry follows. The span's parent is the span
that is current when you call `.ask`.

The crate installs no subscriber. This output is from
`tracing_subscriber::fmt().with_span_events(FmtSpan::CLOSE)`, which prints each span when it
closes (some fields left out here):

```text
INFO ask{gen_ai.request.model="jev-latest" gen_ai.response.model="jev-1.14.0"
         gen_ai.usage.input_tokens=293 gen_ai.usage.output_tokens=21
         system2.cached=false system2.latency_ms=184
         system2.request_id="req_01a1ce54e294763290b45e66f1f22bf1"}
```

To read the same facts in code, call [`.ask_with_receipt`][Ask::ask_with_receipt] in place of
`.ask`. It returns the answers and a [`Receipt`][Receipt], with the model that answered, the
tokens, the latency, whether the answer came from the cache, and the request ID:

```rust,ignore
let (urgent, receipt) = noul!("Is the ticket urgent?").ask_with_receipt(&ticket).await?;
println!("{} tokens, {:?}", receipt.usage.input_tokens, receipt.latency);
```

Each answer is `Serialize`, with its probabilities, so you can log it or keep it as a label. An
option serializes as its name, so your values need no `Serialize`. `trace_bodies(true)` on a
client also writes the body of each request in a `TRACE` event. A body holds the state, which is
often private, so it is off by default.

The span records both the model that the request asked for (`jev-latest`) and the model that
answered (`jev-1.14.0`).

## Errors

Each error that a call returns converts into one [`system2::Error`][Error], so `?` works anywhere
that returns `system2::Result`. Here one `?` takes an `InvalidChoice` and the next an `AskError`:

```rust
use system2::prelude::*;
use system2::{AskError, Choice, Error};

async fn route(ticket: &str, teams: Vec<String>) -> system2::Result<String> {
    let question = Choice::new("Which team should handle this ticket?", teams)?;
    let team = question.ask(ticket).await?;
    Ok(team.most_likely().clone())
}
```

To handle one failure, match through it:

```rust,ignore
match route(ticket, teams).await {
    Ok(team) => println!("{team}"),
    Err(Error::Question(error)) => eprintln!("fix the team list: {error}"),
    Err(Error::Ask(AskError::Offline)) => eprintln!("no recorded answer for this ticket"),
    Err(error) => eprintln!("{error}"),
}
```

`Error::Question` holds a [`QuestionError`][QuestionError], from whichever constructor refused
the question. Each message says what failed, and the fix when there is one. `AskError::Http` and
`AskError::InvalidResponse` carry the request ID, when the API sent one, for TypeSafe support.

## Blocking API

[`system2::blocking`][blocking] asks with no `async` and needs no Tokio in your crate. Import
`system2::blocking::prelude::*` in place of `system2::prelude::*`, and drop each `.await`:

```rust,no_run
use system2::blocking::prelude::*;

fn main() -> system2::Result<()> {
    let urgent = noul!("Is the ticket urgent?").ask("I cannot log in.")?;
    println!("urgent: {:.2}", urgent.probability());
    Ok(())
}
```

The questions, sets, and answers are the same in both modes, and both use the same global
client. `blocking::Client::from(client)` wraps a `Client` with your settings. A blocking `ask`
inside an async runtime panics, so pick one mode for each program.

## Limits

TypeSafe's API sets most of these, and the crate checks each one as early as it can.

| Limit | Value | Checked by |
| --- | --- | --- |
| Options | 1 to 255 for a Choice, each with a different, nonempty name | Compiler or constructor |
| Levels | 1 to 10 for a Score, each different | Compiler or constructor |
| Tuples | 1 to 12 items, each a question or a set | Compiler |
| Tokens | About 32k for the state and the longest question together | `AskError::TokenLimit` |
| Request | About 64k tokens | `AskError::TokenLimit` |
| Images | PNG or JPEG, at most 16 MiB, each side 280 to 1568 px | `InvalidImage` |
| Image bytes | At most 32 MiB of images in a request | `AskError::ByteLimit` |
| Bbox | Jev 1.14 or a later model | `AskError::BboxModel` |
| Precision | 0.01 for each probability and each edge | |

Arrays and `Vec`s of questions have no limit. `long_context(true)` raises each token limit to
about 262k. TypeSafe can change images and Bbox in a new release of Jev.

## Crate features

- `image`: `Image::new` and `image!` also take WebP and GIF, turn each image upright, and scale
  it to fit the limits. `Image::crop` cuts a region from the original, and `Image` converts to
  and from `image::DynamicImage`. Without it, an image must be a PNG or JPEG that already fits.

The minimum supported Rust version is 1.88. TLS is rustls only, and the crate has no `unsafe`
code.

## The main types

- Questions: [`Noul`][Noul], [`Choice`][Choice], [`Score`][Score], and [`Bbox`][Bbox]. Their
  answers are [`NoulAnswer`][NoulAnswer], [`ChoiceAnswer`][ChoiceAnswer],
  [`ScoreAnswer`][ScoreAnswer], and [`BboxAnswer`][BboxAnswer].
- Sets: tuples, arrays, `Vec`, `Option`, [`questions!`][questions!], and
  [`Questions`][Questions] for a struct that derives [`Answers`][Answers].
- [`Client`][Client] holds the key, the model, the cache, and the other settings of a request.
- [`Image`][Image] is an image for a state, a question, or an option.
- [`Receipt`][Receipt] has the model, the tokens, and the latency of one request.
- [`Error`][Error] covers every error, and [`AskError`][AskError] tells why a request failed.

## Examples

Each example is a crate of its own in [`examples/`][examples]. Run one with `cargo run -p <name>`,
or replay its recording with no key, as CI does:

```sh
SYSTEM2_CACHE=examples/recordings/triage.jsonl SYSTEM2_OFFLINE=1 cargo run -p triage
```

- [`quick_start`][quick_start]: asks one yes-or-no question about a short function, and reads the
  probability of yes.
- [`triage`][triage]: the triage on this page, five tickets with three questions each.
- [`find_in_image`][find_in_image]: finds an object in a photo, and writes a crop of it. It needs
  `--features image`.
- [`citation_check`][citation_check]: a port of a [TypeSafe
  cookbook](https://docs.typesafe.ai/cookbooks). It checks whether its source supports each
  citation, and sends the uncertain ones to a person.

## License

Licensed under either of
[Apache License, Version 2.0](https://github.com/jhartquist/system2/blob/main/LICENSE-APACHE) or
[MIT license](https://github.com/jhartquist/system2/blob/main/LICENSE-MIT) at your option.

Issues are welcome, but pull requests are not accepted for now.

[Noul]: https://docs.rs/system2/latest/system2/struct.Noul.html
[NoulAnswer]: https://docs.rs/system2/latest/system2/struct.NoulAnswer.html
[Ask::ask]: https://docs.rs/system2/latest/system2/trait.Ask.html#method.ask
[Ask::ask_with_receipt]: https://docs.rs/system2/latest/system2/trait.Ask.html#method.ask_with_receipt
[Choice]: https://docs.rs/system2/latest/system2/struct.Choice.html
[Choice::with_values]: https://docs.rs/system2/latest/system2/struct.Choice.html#method.with_values
[ChoiceAnswer]: https://docs.rs/system2/latest/system2/struct.ChoiceAnswer.html
[Score]: https://docs.rs/system2/latest/system2/struct.Score.html
[ScoreAnswer]: https://docs.rs/system2/latest/system2/struct.ScoreAnswer.html
[Bbox]: https://docs.rs/system2/latest/system2/struct.Bbox.html
[BboxAnswer]: https://docs.rs/system2/latest/system2/struct.BboxAnswer.html
[Image]: https://docs.rs/system2/latest/system2/struct.Image.html
[Noul::new]: https://docs.rs/system2/latest/system2/struct.Noul.html#method.new
[Noul::with_criteria]: https://docs.rs/system2/latest/system2/struct.Noul.html#method.with_criteria
[noul!]: https://docs.rs/system2/latest/system2/macro.noul.html
[questions!]: https://docs.rs/system2/latest/system2/macro.questions.html
[Questions]: https://docs.rs/system2/latest/system2/struct.Questions.html
[Answers]: https://docs.rs/system2/latest/system2/derive.Answers.html
[Client]: https://docs.rs/system2/latest/system2/struct.Client.html
[Client::cache]: https://docs.rs/system2/latest/system2/struct.Client.html#method.cache
[Client::from_env]: https://docs.rs/system2/latest/system2/struct.Client.html#method.from_env
[Client::new]: https://docs.rs/system2/latest/system2/struct.Client.html#method.new
[Client::offline]: https://docs.rs/system2/latest/system2/struct.Client.html#method.offline
[Client::set_global]: https://docs.rs/system2/latest/system2/struct.Client.html#method.set_global
[Receipt]: https://docs.rs/system2/latest/system2/struct.Receipt.html
[Error]: https://docs.rs/system2/latest/system2/enum.Error.html
[AskError]: https://docs.rs/system2/latest/system2/enum.AskError.html
[QuestionError]: https://docs.rs/system2/latest/system2/enum.QuestionError.html
[blocking]: https://docs.rs/system2/latest/system2/blocking/index.html
[examples]: https://github.com/jhartquist/system2/tree/main/examples
[quick_start]: https://github.com/jhartquist/system2/tree/main/examples/quick_start
[triage]: https://github.com/jhartquist/system2/tree/main/examples/triage
[find_in_image]: https://github.com/jhartquist/system2/tree/main/examples/find_in_image
[citation_check]: https://github.com/jhartquist/system2/tree/main/examples/citation_check

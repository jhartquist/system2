# system2

[![crates.io](https://img.shields.io/crates/v/system2.svg)](https://crates.io/crates/system2)
[![docs.rs](https://img.shields.io/docsrs/system2)](https://docs.rs/system2)
[![CI](https://github.com/jhartquist/system2/actions/workflows/ci.yml/badge.svg)](https://github.com/jhartquist/system2/actions/workflows/ci.yml)

An unofficial, idiomatic Rust SDK for [TypeSafe](https://typesafe.ai/)'s System One model,
[Jev](https://typesafe.ai/blog/introducing-system-one-models-and-jev).

> the goal is to make System 1 tasks extremely reliable and get System 2 by composing with code
>
> \- [Diogo Almeida](https://x.com/CompleteSkeptic/status/2099981459541143995), CEO of TypeSafe

Daniel Kahneman's *Thinking, Fast and Slow* popularized two modes of thinking:

- **System 1** makes quick, intuitive judgments.
- **System 2** reasons slowly and deliberately.

TypeSafe calls Jev a [System One](https://docs.typesafe.ai/concepts/system-one) model: it answers
narrow questions about an input, called the `state`, with calibrated probabilities. `system2`
brings those judgments into Rust. A question or a state can be as simple as a string, or it can
be built from your own enums and structs, and even images. Either way, an answer hands back the
values you passed in, with probabilities and confidence, so you can compose them into decisions
with code.

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
- **Reliable**: retries, timeouts, and a concurrency limit built in.

## Getting started

[Sign up](https://console.typesafe.ai/home) for TypeSafe, create an
[API key](https://console.typesafe.ai/keys), then:

```sh
cargo add system2
cargo add tokio --features macros,rt-multi-thread
export TYPESAFE_API_KEY=...
```

`system2` is young, so expect breaking changes before 1.0.

## Example

A [`noul`][Noul] is Jev's yes-or-no question, and its answer is the probability of yes. This
example asks two questions in one request, then decides based on their probabilities:

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

Jev answered 0.85 for angry and 0.67 for the refund, so this prints `send to a person`.

[`.ask`][Ask::ask] takes any question or set of questions, and a state, and returns the answers in
the shape of the questions. There is no client to set up. The first `.ask` makes a global client
from `TYPESAFE_API_KEY`, and every later `.ask` shares it.

## Questions and state

A question and a state can each be any `serde::Serialize` value. A state must serialize to text,
an object, or an array, and an empty one fails before any request is sent. Here the question
carries a struct of your own, and the state is JSON. Add serde with
`cargo add serde --features derive` and `cargo add serde_json`:

```rust,ignore
use serde::Serialize;
use serde_json::json;

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

Or your own types. The enum is the list of options, its doc comments describe them to Jev, and
the answer is one of your variants. Each option's name is its variant in snake case, such as
`billing`:

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
value of your own that the answer hands back:

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
that names each one. The answer hands back the value Jev picked, with no lookup:

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

An [`Image`][Image] goes anywhere text does: in a placeholder, in `json!`, in a struct, or as an
option. Here a customer sends a screenshot, and the code compares it to a picture of a known bug
that ships with the app:

```rust,ignore
const KNOWN_BUG: Image = image!("known_bug.png");

let screenshot = Image::new(std::fs::read("screenshot.png")?)?;

let same = noul!("Does {screenshot} show the same error as {KNOWN_BUG}?")
    .ask("The app shows this error when I pay.")
    .await?;

if same.is_likely() {
    let dialog = bbox!("The error dialog.").ask(&screenshot).await?;
    println!("the dialog is at {:?}", dialog.to_pixels(&screenshot));
}
```

`image!` uses `include_bytes!` under the hood and checks the image when the crate builds, so a
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
whole question, its options, or its levels from data, goes through a constructor.

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

A tuple, an array, or a `Vec` of questions is a set, and one request asks the whole set. The
first example was a tuple of two questions. Sets nest, so a tuple can hold a `Vec` with one
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

When a set grows, give its answers names. [`questions!`][questions!] makes a struct with a field
for each question. A [`score`][Score] puts the state on an ordered scale of your levels, lowest
first. Jev knows a level only by its text, so each doc comment is the text of its level, and a
variant with no doc comments uses its name. Ask three things about a ticket in one request, then
decide:

```rust,ignore
#[derive(Debug, PartialEq, Levels)]
enum Urgency { Low, Medium, High }

let triage = questions! {
    angry: noul!("Is the customer angry?"),
    team: choice!("Which team should handle this ticket?", Team),
    urgency: score!("How urgent is this ticket?", Urgency),
}
.ask(&ticket)
.await?;

let team = *triage.team.most_likely();
let urgent = triage.urgency.probability_of(&Urgency::High) > 0.7;
let angry = triage.angry.probability() > 0.8;

// act only when Jev gives the team it picked at least 0.9
let decision = if triage.team.probability() < 0.9 {
    "ask a person".to_string()
} else if urgent || angry {
    format!("page the {team:?} lead")
} else {
    format!("add to the {team:?} queue")
};
```

A score's answer also has `score()`, the mean level, from 0 to the index of the last level, and
`most_likely()`, the level with the highest probability. Levels can be inline too, as text or
with your own values, as in `score!("How long is the fix?", [("A day.", 1), ("A week.", 5)])`.

To return the answers from a function, give them a type of your own. `#[derive(Answers)]` names
a question for each field, and [`Questions`][Questions] asks them. The return type picks the
questions, so `Questions::new()` takes no argument:

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

```text
ticket 1: page the Billing lead      "We were charged twice this month. Please fix it today."
ticket 2: add to the Support queue   "How do I change my avatar?"
ticket 3: page the Engineering lead  "The export page has shown a 500 error since this morning..."
ticket 4: add to the Support queue   "hello??"
ticket 5: ask a person               "The invoice page crashes when I try to pay."
```

Ticket 5 could be billing or engineering. Jev gave engineering 0.80 and billing 0.20, the code
wants 0.90 before it acts, so a person gets it. Each number in that code is yours: the answers
keep their probabilities, so every threshold stays in your code where you can see it.

One answer can also pick the next question:

```rust,ignore
if let Team::Billing = team {
    let refund = noul!("Does the customer ask for a refund?").ask(&ticket).await?;
    // ...
}
```

## Testing

Record the answers once, then replay them with no API key and no network:

```sh
SYSTEM2_CACHE=triage.jsonl cargo run                     # asks Jev, and records
SYSTEM2_CACHE=triage.jsonl SYSTEM2_OFFLINE=1 cargo run   # replays, sends nothing
```

Change the team threshold from 0.9 to 0.8 and replay: ticket 5 pages the engineering lead.
Change it back, and it goes to a person. Trying both cost zero requests. Offline, a request that
changed since the recording (the question, the state, or the model) is an error, not a quiet call
to the API. A replayed answer counts no tokens and has no request ID. The file holds each response
and a hash of its request, not the request itself. Treat it like the rest of your test data.

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

On a client of your own, `Client::from_env()?.cache("tests/jev.jsonl")?.offline(true)` does the
same in code. Two live runs of one request can give different probabilities, and the cache is
what makes them the same. A request matches only when its body matches byte for byte, so use a
`BTreeMap`, not a `HashMap`, in a state: a `HashMap` writes its keys in a different order each
run.

The right number is yours to find. TypeSafe trains Jev so that its probabilities are
[calibrated](https://docs.typesafe.ai/concepts/system-one): across many answers, the stated
probability should match how often such answers are right. It is no guarantee about any one
answer, and the numbers on this page are examples, not advice. Pick yours on tickets you have
already labelled, and weigh what each kind of mistake costs you.

Once you have a number, pin down what your code does at it. A decision is a function of answers,
so test it like any other function. `answer` makes a real answer from the values you give it,
with no key, no network, and no runtime:

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

`answer` goes through the same checks as a response from Jev, so a test cannot build an answer
that Jev could never give: `REFUND.answer(1.2)` is an error. A Choice and a Score have one too.

## Concurrency and retries

Ask about a whole queue at once. By default the client keeps 8 requests in flight, gives each
attempt 10 s, and retries a 408, a 429, a 5xx, a timeout, or a failed connection twice, with
backoff.

To change a setting (`max_concurrent_requests`, `timeout`, `max_retries`), build a
[`Client`][Client] of your own. [`set_global`][Client::set_global] makes it the client that every
`.ask` uses, so the rest of your code does not change. `client.ask(question, state)` uses one
client without making it global.

```rust,ignore
use tokio::task::JoinSet;

let client = Client::from_env()?.max_concurrent_requests(2).set_global();

let mut asking = JoinSet::new();
for ticket in &tickets {
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

The answers arrive in the order they finish, so `.tag` carries each ticket's ID back next to its
answer. The tag is not sent to Jev.

[`Client::from_env`][Client::from_env] reads these variables:

| Variable | Value |
| --- | --- |
| `TYPESAFE_API_KEY` | The API key. |
| `TYPESAFE_BASE_URL` | The base URL. The default is `https://api.typesafe.ai`. |
| `TYPESAFE_DEFAULT_MODEL` | The model. The default is `jev-latest`. |
| `SYSTEM2_CACHE` | The path of a cache file. |
| `SYSTEM2_OFFLINE` | `1` answers only from the cache. |

## Tracing

Each request is a `tracing` span, so it goes to the logs and dashboards you already have. Its
fields have the model that answered, the tokens, the latency, and the request ID, and the
`gen_ai.*` names follow OpenTelemetry's conventions for generative AI. Each attempt is a `DEBUG`
event in the span, and each retry is a `WARN` event with the error and the wait. The five
requests above took 73 to 184 ms.

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

The span above shows both names. The default model is `jev-latest`, which TypeSafe moves to
each new version. `.model("jev-1.14.0")` on the client pins one, so the model changes only when
your code does.

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
the question. Each message says what failed, and the fix when there is one. An `AskError` for a
response from the API carries its request ID, when the API sent one, for TypeSafe support.

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
| Options | 1 to 255 for a Choice, each with a different, nonempty name. | The compiler, or a constructor |
| Levels | 1 to 10 for a Score, each different. | The compiler, or a constructor |
| Tuples | 1 to 12 sets in a tuple. Other sets have no limit. | The compiler |
| Tokens | About 32k for the state and the longest question, and 64k for the request. `long_context(true)` raises each to about 262k. | `AskError::TokenLimit` |
| Images | PNG or JPEG, at most 16 MiB, each side 280 to 1568 px. At most 32 MiB of images in a request. | `InvalidImage`, or `AskError::ByteLimit` |
| Bbox | `jev-1.14.0` or a later model. | `AskError::BboxModel` |
| Precision | The API rounds each probability and each edge to 0.01. | |

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
- [`Receipt`][Receipt] has the model, the tokens, and the time of one request.
- [`Error`][Error] covers every error, and [`AskError`][AskError] tells why a request failed.

## Examples

Each example is a crate of its own in
[`examples/`](https://github.com/jhartquist/system2/tree/main/examples). Run one with
`cargo run -p <name>`.

- [`quick_start`](https://github.com/jhartquist/system2/tree/main/examples/quick_start): asks if
  a function has a bug.
- [`find_in_image`](https://github.com/jhartquist/system2/tree/main/examples/find_in_image):
  finds an object in a photo, and writes a crop of it. It needs `--features image`.
- [`citation_check`](https://github.com/jhartquist/system2/tree/main/examples/citation_check):
  a port of a [TypeSafe cookbook](https://docs.typesafe.ai/cookbooks). It finds the citations
  that are not true, and sends the uncertain ones to a person.

## License

Licensed under either of
[Apache License, Version 2.0](https://github.com/jhartquist/system2/blob/main/LICENSE-APACHE) or
[MIT license](https://github.com/jhartquist/system2/blob/main/LICENSE-MIT) at your option.

Issues are welcome, but pull requests are not accepted for now.

`system2` is not affiliated with or endorsed by TypeSafe.

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
[Answers]: https://docs.rs/system2/latest/system2/trait.Answers.html
[Client]: https://docs.rs/system2/latest/system2/struct.Client.html
[Client::from_env]: https://docs.rs/system2/latest/system2/struct.Client.html#method.from_env
[Client::set_global]: https://docs.rs/system2/latest/system2/struct.Client.html#method.set_global
[Receipt]: https://docs.rs/system2/latest/system2/struct.Receipt.html
[Error]: https://docs.rs/system2/latest/system2/enum.Error.html
[AskError]: https://docs.rs/system2/latest/system2/enum.AskError.html
[QuestionError]: https://docs.rs/system2/latest/system2/enum.QuestionError.html
[blocking]: https://docs.rs/system2/latest/system2/blocking/index.html

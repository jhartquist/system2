# system2

System 2 decisions from System 1 judgments. An idiomatic Rust SDK for
[TypeSafe](https://typesafe.ai)'s System One model, Jev.

> the goal is to make System 1 tasks extremely reliable and get System 2 by composing with code
>
> \- [Diogo Almeida](https://x.com/CompleteSkeptic/status/2099981459541143995), TypeSafe CEO

```rust
use system2::prelude::*;

#[derive(PartialEq, PartialOrd, Levels)]
enum Urgency {
    Low,
    Medium,
    High,
}

#[tokio::main]
async fn main() -> system2::Result<()> {
    let ticket = "I was charged twice. Please help ASAP.";

    let triage = questions! {
        billing: noul!("Is this about billing?"),
        tone: choice!("What is the tone?", ["calm", "angry"]),
        urgency: score!("How urgent is this?", Urgency),
    }
    .ask(ticket)
    .await?;

    if triage.billing.is_likely() && triage.urgency.most_likely() >= &Urgency::High {
        println!("escalate to billing, tone: {}", triage.tone.most_likely());
    }

    Ok(())
}
```

## Roadmap

- [ ] question types
  - [x] `Noul`
  - [x] `Choice`
  - [x] `Score`
  - [ ] `Bbox`
- [x] enums as options and levels
- [ ] question sets
  - [x] tuples
  - [x] `Vec`
  - [x] iterators
  - [x] anonymous sets (`questions! { ... }`)
  - [ ] named sets (`#[derive(Answers)]`)
  - [x] tags
- [x] any serializable state
- [x] compile-time checks for static questions
- [x] run-time checks for dynamic questions
- [ ] client
  - [x] global client
  - [ ] retries
  - [ ] concurrency limit
  - [ ] cache
  - [ ] token counts
  - [ ] tracing
- [ ] images
- [ ] docs
  - [ ] rustdoc
  - [ ] examples
  - [ ] cookbook ports
- [ ] publish 0.1 on crates.io

## License

`MIT OR Apache-2.0`

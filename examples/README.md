# Examples

Each example is a crate of its own. Run one with `cargo run -p <name>`. The examples read
`TYPESAFE_API_KEY` from the environment.

`recordings/` holds the answers of one live run of each example. Replay an example from its
recording, with no key and no network:

```sh
SYSTEM2_CACHE=examples/recordings/<name>.jsonl SYSTEM2_OFFLINE=1 cargo run -p <name>
```

The output in the README of each example comes from its recording, and CI replays each one.

## Short examples

Each one shows one area of the API.

- [`quick_start`](quick_start): asks one yes-or-no question about a short function, and reads
  the probability of yes.
- [`triage`](triage): asks three questions about each of five support tickets in one request,
  and routes each ticket in plain code.
- [`find_in_image`](find_in_image): finds an object in a photo, and writes a crop of it or a
  rectangle around it. It needs `--features image`.

## Cookbook ports

Ports of [TypeSafe cookbooks](https://docs.typesafe.ai/cookbooks), with their data.

- [`citation_check`](citation_check): checks whether its source supports each citation, and
  sends the uncertain ones to a person.

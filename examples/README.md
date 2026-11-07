# Examples

Each example is a crate of its own. Run one with `cargo run -p <name>`. The examples read
`TYPESAFE_API_KEY` from the environment.

## Short examples

Each one shows one area of the API.

- [`quick_start`](quick_start): asks if a function has a bug.
- [`find_in_image`](find_in_image): finds an object in a photo, and writes a crop of it or a
  rectangle around it. It needs `--features image`.

## Cookbook ports

Ports of [TypeSafe cookbooks](https://docs.typesafe.ai/cookbooks), with their data.

- [`citation_check`](citation_check): finds the citations that are not true, and sends the
  uncertain ones to a person.

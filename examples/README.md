# Examples

Each example is a crate of its own. Run one with `cargo run -p <name>`. The examples read
`TYPESAFE_API_KEY` from the environment.

## Cookbook ports

Ports of [TypeSafe cookbooks](https://docs.typesafe.ai/cookbooks), with their data.

- [`citation_check`](citation_check): finds the citations that are not true, and sends the
  uncertain ones to a person.

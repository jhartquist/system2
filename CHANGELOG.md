# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - YYYY-MM-DD

The first release: an unofficial Rust SDK for TypeSafe's System One model, Jev.

### Added

- The four question types: `Noul`, `Choice`, `Score`, and `Bbox`. Each one has macros that the
  compiler checks, and constructors that check data at run time.
- `#[derive(Options)]` and `#[derive(Levels)]` turn your enums into options and levels.
  `#[derive(Answers)]` and `questions!` name the answers of a set.
- Question sets: tuples, arrays, `Vec`, `Option`, and tags, asked in one request.
- Answers that keep their probabilities, and `answer` on each question type to build one in a
  test.
- Images in a state, a question, or an option, with `image!` for files checked at build time.
  The `image` feature converts, scales, and crops images.
- A client with retries, timeouts, a concurrency limit, token usage, and receipts.
- OpenRouter as a second provider of Jev, through the base URL.
- A cache that records answers and replays them offline.
- A `tracing` span for each request, with OpenTelemetry names for generative AI.
- `system2::blocking`, the same API with no `async`.

[Unreleased]: https://github.com/jhartquist/system2/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/jhartquist/system2/releases/tag/v0.1.0

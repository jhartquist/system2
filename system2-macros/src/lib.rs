//! The macros of `system2`.
//!
//! Use them through [`system2`](https://docs.rs/system2), which re-exports each one.

mod answers;
mod bbox;
mod choice;
mod derive;
mod image;
mod json;
mod noul;
mod part;
mod questions;
mod score;

use proc_macro::TokenStream;
use quote::ToTokens;
use syn::{DeriveInput, parse_macro_input};

/// Makes a [`Noul`] that the compiler checks.
///
/// The macro never fails at run time: every mistake is a compile error, pointing at the bad
/// token. The instructions come first, in one of three forms:
///
/// - A string literal. It works as a format string, like `format!`, where each placeholder
///   names a variable, as in `{name}`. The text outside the placeholders can't be only
///   whitespace. A literal with no placeholders makes a constant.
/// - A `&'static str` constant, such as `include_str!("rule.txt")`. `cargo check` validates it.
/// - A JSON literal, `{ … }` or `[ … ]`, that is not empty, with no key repeated. Each string
///   value is a format string. Any other value is a literal, or a variable of a type that
///   always serializes: text, a number, a `bool`, a `serde_json::Value`, a `serde_json::Map`, or
///   a collection or string-keyed map of those.
///
/// The optional criteria come second: an object with a `"true"` key, a `"false"` key, or both,
/// where each criterion takes any of the forms above. If the instructions come from data, use
/// [`Noul::new`] instead.
///
/// # Examples
///
/// ```
/// use system2::{Noul, noul};
///
/// const URGENT: Noul = noul!("Is the ticket urgent?");
/// const REFUND: Noul = noul!("Does the customer ask for a refund?", {
///     "true": "The customer wants the money back.",
///     "false": "The customer wants no money.",
/// });
/// const POLICY: Noul = noul!({
///     "question": "Does the reply obey the policy?",
///     "policy": "A reply never promises a date.",
/// });
///
/// let product = "the mobile app";
/// let about = noul!("Is the ticket about {product}?");
/// ```
///
/// [`Noul`]: ../system2/struct.Noul.html
/// [`Noul::new`]: ../system2/struct.Noul.html#method.new
#[proc_macro]
pub fn noul(input: TokenStream) -> TokenStream {
    let noul = parse_macro_input!(input as noul::Noul);
    noul.into_token_stream().into()
}

/// Makes a [`Choice`] that the compiler checks.
///
/// The instructions come first, in a form that [`noul!`] takes. The options come second:
///
/// - A type that derives [`Options`]. The answer gives a variant.
/// - An object of names and descriptions. The answer gives the name.
/// - An array of names. The answer gives the name.
/// - An array of `(name, value)` or `(name, description, value)`. The answer gives the value.
///
/// Each name is a string literal, so two equal names are a compile error. Each description has
/// a form of the instructions. Each value is a constant expression of any type, and it is not
/// sent. A Choice has 1 to 255 options. For options from data, use [`Choice::new`],
/// [`Choice::with_descriptions`], or [`Choice::with_values`].
///
/// # Examples
///
/// ```
/// use std::cmp::Ordering;
/// use system2::{Choice, Options, choice};
///
/// #[derive(Debug, Clone, Copy, PartialEq, Options)]
/// enum Team {
///     /// Payments, invoices, and refunds.
///     Billing,
///     /// Bugs and outages.
///     Engineering,
/// }
///
/// const TEAM: Choice<Team> = choice!("Which team must answer the ticket?", Team);
/// const NEXT: Choice<&str> = choice!("What must the agent do?", {
///     "reply": "Reply to the customer.",
///     "escalate": "Give the ticket to an engineer.",
/// });
/// const LANGUAGE: Choice<&str> = choice!("Which language is the ticket in?", ["en", "de", "fr"]);
/// const BETTER: Choice<Ordering> = choice!("Which reply is better?", [
///     ("first", "The first reply is better.", Ordering::Less),
///     ("second", "The second reply is better.", Ordering::Greater),
/// ]);
/// ```
///
/// [`noul!`]: ../system2/macro.noul.html
/// [`Choice`]: ../system2/struct.Choice.html
/// [`Choice::new`]: ../system2/struct.Choice.html#method.new
/// [`Choice::with_descriptions`]: ../system2/struct.Choice.html#method.with_descriptions
/// [`Choice::with_values`]: ../system2/struct.Choice.html#method.with_values
/// [`Options`]: ../system2/trait.Options.html
#[proc_macro]
pub fn choice(input: TokenStream) -> TokenStream {
    let choice = parse_macro_input!(input as choice::Choice);
    choice.into_token_stream().into()
}

/// Makes a [`Score`] that the compiler checks.
///
/// The instructions come first, in a form that [`noul!`] takes. The levels come second, from the
/// lowest to the highest:
///
/// - A type that derives [`Levels`]. The answer gives a variant.
/// - An array of levels as text. The answer gives the text.
/// - An array of `(level, value)`. The answer gives the value.
///
/// Each level is a string literal, so two equal levels are a compile error. Each value is a
/// constant expression of any type, and it is not sent. A Score has 1 to 10 levels. For levels
/// from data, use [`Score::new`] or [`Score::with_values`].
///
/// # Examples
///
/// ```
/// use system2::{Levels, Score, score};
///
/// #[derive(Debug, Clone, Copy, PartialEq, Levels)]
/// enum Urgency {
///     Low,
///     Medium,
///     High,
/// }
///
/// const URGENCY: Score<Urgency> = score!("How urgent is the ticket?", Urgency);
/// const SIZE: Score<&str> = score!("How big is the fix?", ["A line.", "A file.", "A module."]);
/// const DAYS: Score<u32> = score!("How long does the fix take?", [("A day.", 1), ("A week.", 5)]);
/// ```
///
/// [`noul!`]: ../system2/macro.noul.html
/// [`Score`]: ../system2/struct.Score.html
/// [`Score::new`]: ../system2/struct.Score.html#method.new
/// [`Score::with_values`]: ../system2/struct.Score.html#method.with_values
/// [`Levels`]: ../system2/trait.Levels.html
#[proc_macro]
pub fn score(input: TokenStream) -> TokenStream {
    let score = parse_macro_input!(input as score::Score);
    score.into_token_stream().into()
}

/// Makes a [`Bbox`] that the compiler checks.
///
/// It takes only the instructions, in a form that [`noul!`] takes. The instructions describe one
/// region of an image in the state. A Bbox needs the model `jev-1.14.0` or a later version. For
/// instructions from data, use [`Bbox::new`].
///
/// # Examples
///
/// ```
/// use system2::{Bbox, bbox};
///
/// const DIALOG: Bbox = bbox!("The error dialog.");
///
/// let button = "Retry";
/// let found = bbox!("The button with the text `{button}`.");
/// ```
///
/// [`noul!`]: ../system2/macro.noul.html
/// [`Bbox`]: ../system2/struct.Bbox.html
/// [`Bbox::new`]: ../system2/struct.Bbox.html#method.new
#[proc_macro]
pub fn bbox(input: TokenStream) -> TokenStream {
    let bbox = parse_macro_input!(input as bbox::Bbox);
    bbox.into_token_stream().into()
}

/// Makes a question set with a name for each question set in it.
///
/// Its answers are a struct with the same field names. A field with only a name is short for the
/// variable of that name, as in a struct literal. Each field must be a question set, and an error
/// at the field tells the fix for each other value. A tag takes other data along. The struct of
/// the answers derives `Debug`, `Clone`, and `Serialize` when the types of its fields do.
/// `questions! {}` sends no request.
///
/// The set has no type name, so it cannot be a constant. A named set can: see [`Answers`].
///
/// # Examples
///
/// ```no_run
/// use system2::prelude::*;
///
/// # async fn run() -> system2::Result<()> {
/// let triage = questions! {
///     urgent: noul!("Is the ticket urgent?"),
///     angry: noul!("Is the customer angry?"),
/// }
/// .ask("The site is down again.")
/// .await?;
/// if triage.urgent.is_likely() && triage.angry.is_likely() { /* … */ }
/// # Ok(())
/// # }
/// ```
///
/// [`Answers`]: ../system2/derive.Answers.html
#[proc_macro]
pub fn questions(input: TokenStream) -> TokenStream {
    let questions = parse_macro_input!(input as questions::Questions);
    questions.into_token_stream().into()
}

#[doc(hidden)]
#[proc_macro]
pub fn include_image(input: TokenStream) -> TokenStream {
    let path = parse_macro_input!(input as syn::LitStr);
    image::include_image(&path).into()
}

/// Derives [`Options`] for an enum, so that its variants are the options of a [`Choice`].
///
/// Each variant must be a unit variant. The name of an option is the name of its variant in snake
/// case, with no `r#`. Its description is the doc comment of the variant, as rustdoc shows it. Two
/// attributes change them:
///
/// - `#[system2(rename = "…")]` gives the name.
/// - `#[system2(description = "…")]` gives the description. Then the derive ignores the doc
///   comment, so the doc comment can hold text for rustdoc only.
///
/// An option with no description sends its name only. The enum has 1 to 255 variants, and each
/// name is different. Each error is a compile error at its variant.
///
/// # Examples
///
/// ```
/// use system2::{Choice, Options, choice};
///
/// #[derive(Debug, Clone, Copy, PartialEq, Options)]
/// enum Team {
///     /// Payments, invoices, and refunds.
///     Billing,
///     /// Bugs and outages.
///     Engineering,
///     /// The team for all other tickets.
///     #[system2(rename = "support", description = "Each other ticket.")]
///     Help,
/// }
///
/// const TEAM: Choice<Team> = choice!("Which team must answer the ticket?", Team);
/// ```
///
/// [`Options`]: ../system2/trait.Options.html
/// [`Choice`]: ../system2/struct.Choice.html
#[proc_macro_derive(Options, attributes(system2))]
pub fn derive_options(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    derive::options(&input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// Derives [`Levels`] for an enum, so that its variants are the levels of a [`Score`].
///
/// The variants are the levels, from the lowest to the highest. Each variant must be a unit
/// variant. The API knows a level only by its text. Thus a level sends its description: the doc
/// comment of the variant, or the value of `#[system2(description = "…")]`. The doc comment is
/// the text that rustdoc shows. If no variant has a description, each level is the name of its
/// variant in snake case, with no `r#`. If only some variants have one, each other variant is a
/// compile error. The enum has 1 to 10 variants, and each level is different.
///
/// # Examples
///
/// ```
/// use system2::{Levels, Score, score};
///
/// #[derive(Debug, Clone, Copy, PartialEq, Levels)]
/// enum Severity {
///     /// Nothing breaks.
///     Minor,
///     /// A feature breaks for some customers.
///     Major,
///     /// The level that pages the engineer on call.
///     #[system2(description = "Data is lost.")]
///     Critical,
/// }
///
/// const SEVERITY: Score<Severity> = score!("How severe is the bug?", Severity);
/// ```
///
/// [`Levels`]: ../system2/trait.Levels.html
/// [`Score`]: ../system2/struct.Score.html
#[proc_macro_derive(Levels, attributes(system2))]
pub fn derive_levels(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    derive::levels(&input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// Derives [`Answers`] for a struct, so that [`Questions`] asks the questions of its fields.
///
/// The struct has named fields, and no generic parameters. Each field names its question set with
/// `#[system2(question = …)]`. The type of the field is the answer set of that question set. The
/// question set must be a constant expression, such as `URGENT` or `noul!("…")`. A named set in a
/// named set is `Questions::new()`. For questions from data, use [`questions!`]. The derive adds
/// no method to the struct. A public struct can hold the answer of a private type, such as a
/// `ChoiceAnswer` of a private enum.
///
/// # Examples
///
/// ```no_run
/// use system2::prelude::*;
/// use system2::{ChoiceAnswer, NoulAnswer};
///
/// #[derive(Debug, Clone, Copy, PartialEq, Options)]
/// enum Team {
///     Billing,
///     Engineering,
/// }
///
/// #[derive(Debug, Answers)]
/// struct Triage {
///     #[system2(question = noul!("Is the ticket urgent?"))]
///     urgent: NoulAnswer,
///     #[system2(question = choice!("Which team must answer the ticket?", Team))]
///     team: ChoiceAnswer<Team>,
/// }
///
/// const TRIAGE: Questions<Triage> = Questions::new();
///
/// # async fn run(ticket: &str) -> system2::Result<()> {
/// let triage = TRIAGE.ask(ticket).await?;
/// if triage.urgent.is_likely() && *triage.team.most_likely() == Team::Engineering { /* … */ }
/// # Ok(())
/// # }
/// ```
///
/// [`Answers`]: ../system2/trait.Answers.html
/// [`Questions`]: ../system2/struct.Questions.html
/// [`questions!`]: ../system2/macro.questions.html
#[proc_macro_derive(Answers, attributes(system2))]
pub fn derive_answers(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    answers::answers(&input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

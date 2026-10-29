use crate::part::{Part, constant_value, name_or_level, parse_type};
use proc_macro2::{Span, TokenStream};
use quote::{ToTokens, quote};
use std::collections::HashMap;
use syn::parse::{Parse, ParseStream};
use syn::{Expr, LitStr, Token, Type, bracketed, parenthesized, token};

const MAX_LEVELS: usize = 10;

const TAKES: &str = "`score!` takes the instructions, then the levels: a type or an array";

const LEVEL_FORMS: &str = "each level must be text, or a tuple of `(level, value)`";

pub(crate) struct Score {
    instructions: Part,
    levels: Levels,
}

// the levels of a type, or the levels written inline
enum Levels {
    Type(Type),
    Inline(Vec<Entry>),
}

struct Entry {
    level: LitStr,
    value: TokenStream,
}

impl Parse for Score {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        if input.is_empty() {
            return Err(syn::Error::new(Span::call_site(), TAKES));
        }
        let instructions = Part::parse(input, "the instructions", "Score::new")?;
        if input.parse::<Option<Token![,]>>()?.is_none() || input.is_empty() {
            return Err(syn::Error::new(Span::call_site(), TAKES));
        }

        let levels = if input.peek(token::Bracket) {
            Levels::Inline(array(input)?)
        } else {
            let message = "the levels must be a type or an array: \
                           for levels from data, use a constructor such as `Score::new`";
            Levels::Type(parse_type(input, message)?)
        };

        input.parse::<Option<Token![,]>>()?;
        if !input.is_empty() {
            let message = "`score!` takes the instructions, then the levels, and nothing more";
            return Err(input.error(message));
        }
        Ok(Score {
            instructions,
            levels,
        })
    }
}

impl ToTokens for Score {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let Score {
            instructions,
            levels,
        } = self;
        let score = match levels {
            Levels::Type(levels) => {
                let levels = quote!(<#levels as ::system2::Levels>::MACRO_LEVELS);
                quote!(::system2::__private::score(#instructions, #levels))
            }
            Levels::Inline(entries) => {
                let rows = entries
                    .iter()
                    .map(|Entry { level, value }| quote!((#level, #value)));
                quote!(::system2::__private::score(#instructions, || ::std::vec![#(#rows),*]))
            }
        };
        tokens.extend(score);
    }
}

fn array(input: ParseStream) -> syn::Result<Vec<Entry>> {
    let content;
    let brackets = bracketed!(content in input);
    let entries = content.parse_terminated(entry, Token![,])?;
    validated(entries.into_iter().collect(), brackets.span.join())
}

// a level, which is also the value, or a tuple of the level and the value
fn entry(input: ParseStream) -> syn::Result<Entry> {
    if input.peek(LitStr) {
        let level = level(input)?;
        let value = level.to_token_stream();
        return Ok(Entry { level, value });
    }
    if !input.peek(token::Paren) {
        return Err(input.error(LEVEL_FORMS));
    }

    let content;
    let parens = parenthesized!(content in input);
    let bad_form = || syn::Error::new(parens.span.join(), LEVEL_FORMS);
    let level = level(&content)?;
    if content.parse::<Option<Token![,]>>()?.is_none() || content.is_empty() {
        return Err(bad_form());
    }
    let value: Expr = content.parse()?;
    content.parse::<Option<Token![,]>>()?;
    if !content.is_empty() {
        return Err(bad_form());
    }

    let value = constant_value(&value);
    Ok(Entry { level, value })
}

fn level(input: ParseStream) -> syn::Result<LitStr> {
    let literal: LitStr = input
        .parse()
        .map_err(|error| syn::Error::new(error.span(), "a level must be a string literal"))?;
    name_or_level(&literal, "a level", "Score::new")
}

// the brackets take the error when there is no level
fn validated(entries: Vec<Entry>, brackets: Span) -> syn::Result<Vec<Entry>> {
    let count = entries.len();
    if !(1..=MAX_LEVELS).contains(&count) {
        let span = entries.get(MAX_LEVELS).map_or(brackets, |e| e.level.span());
        let message = format!("a Score must have 1 to {MAX_LEVELS} levels, not {count}");
        return Err(syn::Error::new(span, message));
    }
    let mut indexes = HashMap::new();
    for (second, entry) in entries.iter().enumerate() {
        if let Some(first) = indexes.insert(entry.level.value(), second) {
            let message = format!(
                "each level must be different, but the levels at index {first} and {second} \
                 are the same"
            );
            return Err(syn::Error::new(entry.level.span(), message));
        }
    }
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::Score;
    use proc_macro2::TokenStream;
    use quote::{ToTokens, quote};

    fn expanded(tokens: TokenStream) -> String {
        let score: Score = syn::parse2(tokens).unwrap();
        score.to_token_stream().to_string()
    }

    fn text(literal: &str) -> TokenStream {
        quote! {
            ::system2::__private::Json::String(::std::borrow::Cow::Borrowed(
                ::system2::__private::unescaped(::core::format_args!(#literal))
            ))
        }
    }

    // a value as the expansion writes it
    fn value(value: &TokenStream) -> TokenStream {
        quote!(const { #value })
    }

    #[test]
    fn to_tokens_gives_the_levels_of_a_type() {
        let instructions = text("How severe?");

        assert_eq!(
            expanded(quote!("How severe?", Severity)),
            quote!(::system2::__private::score(
                #instructions,
                <Severity as ::system2::Levels>::MACRO_LEVELS
            ))
            .to_string()
        );
    }

    #[test]
    fn to_tokens_makes_the_levels_in_a_function() {
        let instructions = text("How long?");
        let day = value(&quote!("day"));

        assert_eq!(
            expanded(quote!("How long?", ["A {{line}}.", ("A day.", "day"),],)),
            quote! {
                ::system2::__private::score(#instructions, || ::std::vec![
                    ("A {line}.", "A {line}."),
                    ("A day.", #day)
                ])
            }
            .to_string()
        );
    }

    #[test]
    fn parse_refuses_bad_input() {
        let takes = "`score!` takes the instructions, then the levels: a type or an array";
        let from_data = "the levels must be a type or an array: \
                         for levels from data, use a constructor such as `Score::new`";
        let item = "each level must be text, or a tuple of `(level, value)`";
        let levels = (0..11).map(|index| format!("Level {index}."));
        let cases = [
            (quote!(), takes),
            (quote!("How big?"), takes),
            (quote!("How big?", sizes.iter()), from_data),
            (quote!("How big?", include_str!("levels.json")), from_data),
            (quote!("How big?", { "small": "A line." }), from_data),
            (
                quote!("How big?", []),
                "a Score must have 1 to 10 levels, not 0",
            ),
            (
                quote!("How big?", [#(#levels),*]),
                "a Score must have 1 to 10 levels, not 11",
            ),
            (
                quote!("How big?", ["A line.", "A file.", "A line."]),
                "each level must be different, but the levels at index 0 and 2 are the same",
            ),
            (quote!("How big?", [" "]), "a level must not be empty"),
            (
                quote!("How big?", ["{count} lines."]),
                "a level must have no placeholder: for text from data, use `Score::new`",
            ),
            (quote!("How big?", [size]), item),
            (quote!("How big?", [("A day.")]), item),
            (quote!("How big?", [("A day.", "One day.", 1)]), item),
            (
                quote!("How big?", [(day, 1)]),
                "a level must be a string literal",
            ),
            (
                quote!("How big?", Size, Extra),
                "`score!` takes the instructions, then the levels, and nothing more",
            ),
        ];
        for (tokens, message) in cases {
            let error = syn::parse2::<Score>(tokens.clone()).err().unwrap();
            assert_eq!(error.to_string(), message, "{tokens}");
        }
    }

    // rustc refuses a path that is not a type, such as a variable
    #[test]
    fn parse_takes_each_path_as_a_type() {
        let cases = [
            quote!(V2),
            quote!(r#Severity),
            quote!(crate::review::Severity),
            quote!(T),
            quote!(sizes),
            quote!(MAX_RISK),
        ];
        for levels in cases {
            assert!(
                syn::parse2::<Score>(quote!("How big?", #levels)).is_ok(),
                "{levels}"
            );
        }
    }

    #[test]
    fn parse_takes_1_to_10_levels() {
        for count in [1, 10] {
            let levels = (0..count).map(|index| format!("Level {index}."));
            let score = syn::parse2::<Score>(quote!("How big?", [#(#levels),*]));
            assert!(score.is_ok(), "{count}");
        }
    }
}

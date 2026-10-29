use crate::part::{Part, constant_value, name_or_level, optional, parse_type};
use proc_macro2::{Group, Span, TokenStream};
use quote::{ToTokens, quote};
use std::collections::HashSet;
use syn::parse::{Parse, ParseStream, Parser};
use syn::{Expr, LitStr, Token, Type, braced, bracketed, parenthesized, token};

const MAX_OPTIONS: usize = 255;

const TAKES: &str =
    "`choice!` takes the instructions, then the options: a type, an object, or an array";

const OPTION_FORMS: &str =
    "each option must be a name, or a tuple of `(name, value)` or `(name, description, value)`";

pub(crate) struct Choice {
    instructions: Part,
    options: Options,
}

// the options of a type, or the options written inline
enum Options {
    Type(Type),
    Inline(Vec<Entry>),
}

struct Entry {
    name: LitStr,
    description: Option<Part>,
    value: TokenStream,
}

impl Parse for Choice {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        if input.is_empty() {
            return Err(syn::Error::new(Span::call_site(), TAKES));
        }
        let instructions = Part::parse(input, "the instructions", "Choice::new")?;
        if input.parse::<Option<Token![,]>>()?.is_none() || input.is_empty() {
            return Err(syn::Error::new(Span::call_site(), TAKES));
        }

        let options = if input.peek(token::Brace) {
            Options::Inline(object(input)?)
        } else if input.peek(token::Bracket) {
            Options::Inline(array(input)?)
        } else {
            let message = "the options must be a type, an object, or an array: \
                           for options from data, use a constructor such as `Choice::new`";
            Options::Type(parse_type(input, message)?)
        };

        input.parse::<Option<Token![,]>>()?;
        if !input.is_empty() {
            let message = "`choice!` takes the instructions, then the options, and nothing more";
            return Err(input.error(message));
        }
        Ok(Choice {
            instructions,
            options,
        })
    }
}

impl ToTokens for Choice {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let Choice {
            instructions,
            options,
        } = self;
        let choice = match options {
            Options::Type(options) => {
                let options = quote!(<#options as ::system2::Options>::MACRO_OPTIONS);
                quote!(::system2::__private::choice(#instructions, #options))
            }
            Options::Inline(entries) => {
                let rows = entries.iter().map(|entry| {
                    let Entry {
                        name,
                        description,
                        value,
                    } = entry;
                    let description = optional(description.as_ref());
                    quote!((#name, #description, #value))
                });

                // a `fn` cannot hold a description from run time
                let descriptions = entries.iter().filter_map(|e| e.description.as_ref());
                if descriptions.into_iter().all(Part::is_const) {
                    quote!(::system2::__private::choice(#instructions, || ::std::vec![#(#rows),*]))
                } else {
                    let rows = quote!(::std::vec![#(#rows),*]);
                    quote!(::system2::__private::owned_choice(#instructions, #rows))
                }
            }
        };
        tokens.extend(choice);
    }
}

// each name with its description, and the name is the value
fn object(input: ParseStream) -> syn::Result<Vec<Entry>> {
    let content;
    let braces = braced!(content in input);
    let mut entries = Vec::new();
    while !content.is_empty() {
        let name = name(&content)?;
        content.parse::<Token![:]>()?;
        let description = Some(description(&content, &name)?);
        let value = name.to_token_stream();
        entries.push(Entry {
            name,
            description,
            value,
        });
        if content.is_empty() {
            break;
        }
        content.parse::<Token![,]>()?;
    }
    validated(entries, braces.span.join())
}

fn array(input: ParseStream) -> syn::Result<Vec<Entry>> {
    let content;
    let brackets = bracketed!(content in input);
    let entries = content.parse_terminated(entry, Token![,])?;
    validated(entries.into_iter().collect(), brackets.span.join())
}

// a name, which is also the value, or a tuple of the name, a description, and the value
fn entry(input: ParseStream) -> syn::Result<Entry> {
    if input.peek(LitStr) {
        let name = name(input)?;
        let value = name.to_token_stream();
        let description = None;
        return Ok(Entry {
            name,
            description,
            value,
        });
    }
    if !input.peek(token::Paren) {
        return Err(input.error(OPTION_FORMS));
    }

    let content;
    let parens = parenthesized!(content in input);
    let bad_form = || syn::Error::new(parens.span.join(), OPTION_FORMS);
    let name = name(&content)?;
    if content.parse::<Option<Token![,]>>()?.is_none() || content.is_empty() {
        return Err(bad_form());
    }

    // a json description such as `{ "rule": rule }` is not an expression, so it goes as tokens
    let is_expression = content.fork().parse::<Expr>().is_ok();
    let is_group = content.peek(token::Brace) || content.peek(token::Bracket);
    let second_item = if !is_expression && is_group {
        content.parse::<Group>()?.into_token_stream()
    } else {
        content.parse::<Expr>()?.into_token_stream()
    };
    let has_third_item = content.parse::<Option<Token![,]>>()?.is_some() && !content.is_empty();
    let explicit_value = if has_third_item {
        Some(content.parse::<Expr>()?)
    } else {
        None
    };
    content.parse::<Option<Token![,]>>()?;
    if !content.is_empty() {
        return Err(bad_form());
    }

    let (description, value) = match explicit_value {
        Some(value) => {
            let parse = |input: ParseStream| description(input, &name);
            (Some(parse.parse2(second_item)?), value)
        }
        None if is_json_object.parse2(second_item.clone()).unwrap_or(false) => {
            let message =
                "a description needs a third item in the tuple: use `(name, description, value)`";
            return Err(syn::Error::new_spanned(second_item, message));
        }
        None => (None, syn::parse2(second_item)?),
    };

    let value = constant_value(&value);
    Ok(Entry {
        name,
        description,
        value,
    })
}

fn name(input: ParseStream) -> syn::Result<LitStr> {
    let literal: LitStr = input.parse().map_err(|error| {
        syn::Error::new(
            error.span(),
            "the name of an option must be a string literal",
        )
    })?;
    name_or_level(&literal, "the name of an option", "Choice::new")
}

fn description(input: ParseStream, name: &LitStr) -> syn::Result<Part> {
    let part_name = format!("the description of the option `{}`", name.value());
    Part::parse(input, &part_name, "Choice::with_descriptions")
}

// a json object in place of a value is a description with no value
fn is_json_object(input: ParseStream) -> syn::Result<bool> {
    let content;
    braced!(content in input);
    let object = content.peek(LitStr) && content.peek2(Token![:]);
    content.parse::<TokenStream>()?;
    Ok(object)
}

// the brackets take the error when there is no option
fn validated(entries: Vec<Entry>, brackets: Span) -> syn::Result<Vec<Entry>> {
    let count = entries.len();
    if !(1..=MAX_OPTIONS).contains(&count) {
        let span = entries.get(MAX_OPTIONS).map_or(brackets, |e| e.name.span());
        let message = format!("a Choice must have 1 to {MAX_OPTIONS} options, not {count}");
        return Err(syn::Error::new(span, message));
    }
    let mut names = HashSet::new();
    if let Some(entry) = entries.iter().find(|e| !names.insert(e.name.value())) {
        let message = format!(
            "each option must have a different name, but two options have the name `{}`",
            entry.name.value()
        );
        return Err(syn::Error::new(entry.name.span(), message));
    }
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::Choice;
    use proc_macro2::TokenStream;
    use quote::{ToTokens, format_ident, quote};

    fn expanded(tokens: TokenStream) -> String {
        let choice: Choice = syn::parse2(tokens).unwrap();
        choice.to_token_stream().to_string()
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
    fn to_tokens_gives_the_options_of_a_type() {
        let instructions = text("Which problem?");

        assert_eq!(
            expanded(quote!("Which problem?", Problem)),
            quote!(::system2::__private::choice(
                #instructions,
                <Problem as ::system2::Options>::MACRO_OPTIONS
            ))
            .to_string()
        );
    }

    #[test]
    fn to_tokens_makes_the_options_in_a_function() {
        let instructions = text("Which one?");
        let description = text("The first.");
        let none = quote!(::core::option::Option::None);
        let a = value(&quote!("a"));
        let first = value(&quote!(FIRST));

        assert_eq!(
            expanded(quote!(
                "Which one?",
                ["rust", ("a{{b}}", "a"), ("first", "The first.", FIRST),]
            )),
            quote! {
                ::system2::__private::choice(#instructions, || ::std::vec![
                    ("rust", #none, "rust"),
                    ("a{b}", #none, #a),
                    ("first", ::core::option::Option::Some(#description), #first)
                ])
            }
            .to_string()
        );
    }

    #[test]
    fn to_tokens_takes_a_value_that_starts_with_a_bracket_or_a_brace() {
        let instructions = text("Which one?");
        let description = quote!(::system2::__private::Json::Raw(
            ::std::borrow::Cow::Borrowed("[{\"a\":1}]")
        ));
        let none = quote!(::core::option::Option::None);
        let first = value(&quote!([10, 20][0]));
        let second = value(&quote!({ (40, 0) }.0));
        let third = value(&quote!(30));

        assert_eq!(
            expanded(quote!(
                "Which one?",
                [
                    ("first", [10, 20][0]),
                    ("second", { (40, 0) }.0),
                    ("third", [{ "a": 1 }], 30),
                ]
            )),
            quote! {
                ::system2::__private::choice(#instructions, || ::std::vec![
                    ("first", #none, #first),
                    ("second", #none, #second),
                    ("third", ::core::option::Option::Some(#description), #third)
                ])
            }
            .to_string()
        );
    }

    #[test]
    fn to_tokens_gives_the_options_owned_for_a_description_from_run_time() {
        let instructions = text("Which one?");
        let json = quote!(::system2::__private::Json);
        let description = quote! {
            #json::String(::std::borrow::Cow::Owned(::std::format!("Use {lib}.")))
        };

        assert_eq!(
            expanded(quote!("Which one?", { "approve": "Use {lib}." },)),
            quote! {
                ::system2::__private::owned_choice(#instructions, ::std::vec![
                    ("approve", ::core::option::Option::Some(#description), "approve")
                ])
            }
            .to_string()
        );
    }

    #[test]
    fn parse_refuses_bad_input() {
        let takes = "`choice!` takes the instructions, then the options: \
                     a type, an object, or an array";
        let from_data = "the options must be a type, an object, or an array: \
                         for options from data, use a constructor such as `Choice::new`";
        let item = "each option must be a name, \
                    or a tuple of `(name, value)` or `(name, description, value)`";
        let names = (0..256).map(|index| format!("o{index}"));
        let cases = [
            (quote!(), takes),
            (quote!("Which one?"), takes),
            (quote!("Which one?",), takes),
            (quote!("Which one?", teams.iter()), from_data),
            (
                quote!("Which one?", include_str!("options.json")),
                from_data,
            ),
            (
                quote!("{name}", Problem),
                "the instructions must have text outside the placeholders: \
                 for text from data, use `Choice::new`",
            ),
            (
                quote!("Which one?", []),
                "a Choice must have 1 to 255 options, not 0",
            ),
            (
                quote!("Which one?", {}),
                "a Choice must have 1 to 255 options, not 0",
            ),
            (
                quote!("Which one?", [#(#names),*]),
                "a Choice must have 1 to 255 options, not 256",
            ),
            (
                quote!("Which one?", ["go", "rust", "go"]),
                "each option must have a different name, but two options have the name `go`",
            ),
            (
                quote!("Which one?", { "{{": "A brace.", "{{": "The same brace." }),
                "each option must have a different name, but two options have the name `{`",
            ),
            (
                quote!("Which one?", [" "]),
                "the name of an option must not be empty",
            ),
            (
                quote!("Which one?", ["{language}"]),
                "the name of an option must have no placeholder: \
                 for text from data, use `Choice::new`",
            ),
            (
                quote!("Which one?", ["a } b"]),
                "each `}` in text must be written two times, as `}}`",
            ),
            (quote!("Which one?", [rust]), item),
            (quote!("Which one?", [("rust")]), item),
            (quote!("Which one?", [("rust", 1, 2, 3)]), item),
            (
                quote!("Which one?", [("rust", +)]),
                "expected an expression",
            ),
            (
                quote!("Which one?", [(rust, 1)]),
                "the name of an option must be a string literal",
            ),
            (
                quote!("Which one?", { approve: "Approve it." }),
                "the name of an option must be a string literal",
            ),
            (
                quote!("Which one?", [("test", { "rule": rule })]),
                "a description needs a third item in the tuple: \
                 use `(name, description, value)`",
            ),
            (
                quote!("Which one?", { "approve": "" }),
                "the description of the option `approve` must not be empty",
            ),
            (
                quote!("Which one?", [("approve", "{why}", 1)]),
                "the description of the option `approve` must have text outside the \
                 placeholders: for text from data, use `Choice::with_descriptions`",
            ),
            (
                quote!("Which one?", { "approve" "Approve it." }),
                "expected `:`",
            ),
            (
                quote!("Which one?", Problem, Extra),
                "`choice!` takes the instructions, then the options, and nothing more",
            ),
        ];
        for (tokens, message) in cases {
            let error = syn::parse2::<Choice>(tokens.clone()).err().unwrap();
            assert_eq!(error.to_string(), message, "{tokens}");
        }
    }

    // rustc refuses a path that is not a type, such as a variable
    #[test]
    fn parse_takes_each_path_as_a_type() {
        let cases = [
            quote!(V2),
            quote!(r#Problem),
            quote!(crate::review::Problem),
            quote!(T),
            quote!(teams),
            quote!(MAX_RISK),
        ];
        for options in cases {
            assert!(
                syn::parse2::<Choice>(quote!("Which one?", #options)).is_ok(),
                "{options}"
            );
        }
    }

    #[test]
    fn parse_takes_1_to_255_options() {
        for count in [1, 255] {
            let names = (0..count).map(|index| format_ident!("o{index}").to_string());
            let choice = syn::parse2::<Choice>(quote!("Which one?", [#(#names),*]));
            assert!(choice.is_ok(), "{count}");
        }
    }
}

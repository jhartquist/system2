use crate::part::{Part, optional};
use proc_macro2::{Span, TokenStream};
use quote::{ToTokens, quote};
use syn::parse::{Parse, ParseStream};
use syn::{LitStr, Token, braced, token};

const CRITERION_KEY: &str = r#"each key of the criteria must be "true" or "false""#;

pub(crate) struct Noul {
    instructions: Part,
    true_criterion: Option<Part>,
    false_criterion: Option<Part>,
}

impl Parse for Noul {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        if input.is_empty() {
            let message =
                "`noul!` takes the instructions: a string literal, a constant, or a JSON literal";
            return Err(syn::Error::new(Span::call_site(), message));
        }
        let instructions = Part::parse(input, "the instructions", "Noul::new")?;

        let mut true_criterion = None;
        let mut false_criterion = None;
        if input.parse::<Option<Token![,]>>()?.is_some() && !input.is_empty() {
            if !input.peek(token::Brace) {
                let message = "the criteria must be an object with the key `true`, \
                               the key `false`, or both: to fill in the instructions, \
                               name each value in a placeholder, as in `{name}`";
                return Err(input.error(message));
            }
            let content;
            let braces = braced!(content in input);
            if content.is_empty() {
                let message = "the criteria must have the key `true`, the key `false`, or both";
                return Err(syn::Error::new(braces.span.join(), message));
            }
            while !content.is_empty() {
                let key: LitStr = content
                    .parse()
                    .map_err(|error| syn::Error::new(error.span(), CRITERION_KEY))?;
                let (criterion, name) = match key.value().as_str() {
                    "true" => (&mut true_criterion, "the `true` criterion"),
                    "false" => (&mut false_criterion, "the `false` criterion"),
                    _ => return Err(syn::Error::new(key.span(), CRITERION_KEY)),
                };
                if criterion.is_some() {
                    let message = format!("the key `{}` must occur one time at most", key.value());
                    return Err(syn::Error::new(key.span(), message));
                }
                content.parse::<Token![:]>()?;
                *criterion = Some(Part::parse(&content, name, "Noul::with_criteria")?);
                if content.is_empty() {
                    break;
                }
                content.parse::<Token![,]>()?;
            }
            input.parse::<Option<Token![,]>>()?;
        }

        if !input.is_empty() {
            let message = "`noul!` takes the instructions, then the criteria, and nothing more";
            return Err(input.error(message));
        }
        Ok(Noul {
            instructions,
            true_criterion,
            false_criterion,
        })
    }
}

impl ToTokens for Noul {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let Noul {
            instructions,
            true_criterion,
            false_criterion,
        } = self;
        let true_criterion = optional(true_criterion.as_ref());
        let false_criterion = optional(false_criterion.as_ref());
        tokens.extend(quote! {
            ::system2::__private::noul(#instructions, #true_criterion, #false_criterion)
        });
    }
}

#[cfg(test)]
mod tests {
    use super::Noul;
    use quote::{ToTokens, quote};

    #[test]
    fn to_tokens_gives_each_criterion() {
        let noul: Noul = syn::parse2(quote!("Is it spam?", { "false": "It is a reply." })).unwrap();

        let json = quote!(::system2::__private::Json);
        let cow = quote!(::std::borrow::Cow);
        let text = |literal| {
            quote!(#json::String(#cow::Borrowed(
                ::system2::__private::unescaped(::core::format_args!(#literal))
            )))
        };
        let instructions = text("Is it spam?");
        let false_criterion = text("It is a reply.");
        assert_eq!(
            noul.to_token_stream().to_string(),
            quote! {
                ::system2::__private::noul(
                    #instructions,
                    ::core::option::Option::None,
                    ::core::option::Option::Some(#false_criterion)
                )
            }
            .to_string()
        );
    }

    #[test]
    fn parse_refuses_bad_input() {
        let cases = [
            (
                quote!(),
                "`noul!` takes the instructions: a string literal, a constant, or a JSON literal",
            ),
            (quote!(" "), "the instructions must not be empty"),
            (quote!({}), "the instructions must not be empty"),
            (quote!([]), "the instructions must not be empty"),
            (
                quote!("{name}"),
                "the instructions must have text outside the placeholders: \
                 for text from data, use `Noul::new`",
            ),
            (
                quote!(format!("Is {name} tested?")),
                "the instructions must be a literal, not `format!`: \
                 a literal is a format string, as in `\"Is {name} tested?\"`",
            ),
            (
                quote!("Is `{}` tested?", name),
                "the criteria must be an object with the key `true`, the key `false`, or both: \
                 to fill in the instructions, name each value in a placeholder, as in `{name}`",
            ),
            (
                quote!("Is it spam?", {}),
                "the criteria must have the key `true`, the key `false`, or both",
            ),
            (
                quote!("Is it spam?", { "maybe": "x" }),
                r#"each key of the criteria must be "true" or "false""#,
            ),
            (
                quote!("Is it spam?", { true: "x" }),
                r#"each key of the criteria must be "true" or "false""#,
            ),
            (
                quote!("Is it spam?", { "true": "x", "true": "y" }),
                "the key `true` must occur one time at most",
            ),
            (
                quote!("Is it spam?", { "true": "" }),
                "the `true` criterion must not be empty",
            ),
            (
                quote!("Is it spam?", { "false": " {x} " }),
                "the `false` criterion must have text outside the placeholders: \
                 for text from data, use `Noul::with_criteria`",
            ),
            (
                quote!("Is it spam?", { "true": "x" "false": "y" }),
                "expected `,`",
            ),
            (
                quote!("Is it spam?", { "true": "x" }, "y"),
                "`noul!` takes the instructions, then the criteria, and nothing more",
            ),
        ];
        for (tokens, message) in cases {
            let error = syn::parse2::<Noul>(tokens.clone()).err().unwrap();
            assert_eq!(error.to_string(), message, "{tokens}");
        }
    }
}

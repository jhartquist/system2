use crate::part::Part;
use proc_macro2::{Span, TokenStream};
use quote::{ToTokens, quote};
use syn::Token;
use syn::parse::{Parse, ParseStream};

pub(crate) struct Bbox {
    instructions: Part,
}

impl Parse for Bbox {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        if input.is_empty() {
            let message =
                "`bbox!` takes the instructions: a string literal, a constant, or a JSON literal";
            return Err(syn::Error::new(Span::call_site(), message));
        }
        let instructions = Part::parse(input, "the instructions", "Bbox::new")?;

        input.parse::<Option<Token![,]>>()?;
        if !input.is_empty() {
            let message = "`bbox!` takes the instructions, and nothing more: \
                           to fill in the instructions, name each value in a placeholder, \
                           as in `{name}`";
            return Err(input.error(message));
        }
        Ok(Bbox { instructions })
    }
}

impl ToTokens for Bbox {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let Bbox { instructions } = self;
        tokens.extend(quote! { ::system2::__private::bbox(#instructions) });
    }
}

#[cfg(test)]
mod tests {
    use super::Bbox;
    use quote::{ToTokens, quote};

    #[test]
    fn to_tokens_gives_the_instructions() {
        let bbox: Bbox = syn::parse2(quote!("The mug.",)).unwrap();

        let instructions = quote! {
            ::system2::__private::Json::String(::std::borrow::Cow::Borrowed(
                ::system2::__private::unescaped(::core::format_args!("The mug."))
            ))
        };
        assert_eq!(
            bbox.to_token_stream().to_string(),
            quote!(::system2::__private::bbox(#instructions)).to_string()
        );
    }

    #[test]
    fn parse_refuses_bad_input() {
        let cases = [
            (
                quote!(),
                "`bbox!` takes the instructions: a string literal, a constant, or a JSON literal",
            ),
            (quote!(""), "the instructions must not be empty"),
            (
                quote!("{thing}"),
                "the instructions must have text outside the placeholders: \
                 for text from data, use `Bbox::new`",
            ),
            (
                quote!("The {}.", thing),
                "`bbox!` takes the instructions, and nothing more: \
                 to fill in the instructions, name each value in a placeholder, as in `{name}`",
            ),
        ];
        for (tokens, message) in cases {
            let error = syn::parse2::<Bbox>(tokens.clone()).err().unwrap();
            assert_eq!(error.to_string(), message, "{tokens}");
        }
    }
}

use crate::json::Value;
use proc_macro2::{Group, TokenStream};
use quote::{ToTokens, quote};
use std::str::Chars;
use syn::parse::ParseStream;
use syn::{Expr, ExprLit, ExprMacro, Lit, LitStr, token};

// one part of a question, such as the instructions or a criterion
pub(crate) struct Part {
    // such as "the instructions", for the messages
    name: &'static str,
    form: Form,
}

// how the caller writes a part, in one of the forms that the macro can prove valid
enum Form {
    // a format string, such as "Is `{name}` tested?"
    Literal(LitStr),
    // a constant expression, such as `PURE_TEXT` or `include_str!("prompt.txt")`, sent as written
    Constant(Expr),
    // a json literal, such as `{ "question": "…", "rule": rule }`
    Json(Value),
}

impl Part {
    // `constructor` takes the part from data, as the fix in a message
    pub(crate) fn parse(
        input: ParseStream,
        name: &'static str,
        constructor: &str,
    ) -> syn::Result<Self> {
        if input.peek(token::Brace) || input.peek(token::Bracket) {
            let group: Group = input.fork().parse()?;
            if group.stream().is_empty() {
                let message = format!("{name} must not be empty");
                return Err(syn::Error::new(group.span(), message));
            }
            let form = Form::Json(input.parse()?);
            return Ok(Part { name, form });
        }

        let form = match input.parse()? {
            Expr::Lit(ExprLit {
                lit: Lit::Str(literal),
                ..
            }) => {
                if literal.value().trim().is_empty() {
                    let message = format!("{name} must not be empty");
                    return Err(syn::Error::new(literal.span(), message));
                }
                if !has_text_outside_placeholders(&literal.value()) {
                    let message = format!(
                        "{name} must have text outside the placeholders: \
                         for text from data, use `{constructor}`"
                    );
                    return Err(syn::Error::new(literal.span(), message));
                }
                Form::Literal(literal)
            }
            Expr::Macro(format) if is_format(&format) => {
                let message = format!(
                    "{name} must be a literal, not `format!`: \
                     a literal is a format string, as in `\"Is {{name}} tested?\"`"
                );
                return Err(syn::Error::new_spanned(format, message));
            }
            constant => Form::Constant(constant),
        };
        Ok(Part { name, form })
    }
}

impl ToTokens for Part {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let json = quote!(::system2::__private::Json);
        let cow = quote!(::std::borrow::Cow);
        let part = match &self.form {
            Form::Literal(literal) if has_placeholder(&literal.value()) => {
                quote!(#json::String(#cow::Owned(::std::format!(#literal))))
            }

            // rustc unescapes each brace, and validates the literal
            Form::Literal(literal) => quote! {
                #json::String(#cow::Borrowed(
                    ::system2::__private::unescaped(::core::format_args!(#literal))
                ))
            },

            // the length of an array is a constant, so `cargo check` evaluates the assert before
            // `[0]` takes the text, and no name of the expansion can hide a const of the caller
            Form::Constant(constant) => {
                let message = format!("{} must not be empty", self.name);
                quote! {
                    #json::String(#cow::Borrowed([
                        const { #constant };
                        {
                            ::core::assert!(!::system2::__private::is_blank(#constant), #message);
                            1
                        }
                    ][0]))
                }
            }
            Form::Json(value) => value.to_token_stream(),
        };
        tokens.extend(part);
    }
}

// give `format!` a targeted error, as a literal already takes placeholders
fn is_format(format: &ExprMacro) -> bool {
    let last = format.mac.path.segments.last();
    last.is_some_and(|segment| segment.ident == "format")
}

// `{{` is a brace, so each other `{` starts a placeholder
pub(crate) fn has_placeholder(format: &str) -> bool {
    format.replace("{{", "").contains('{')
}

// rustc validates the format string, so this only skips each placeholder
fn has_text_outside_placeholders(format: &str) -> bool {
    let mut chars = format.chars();
    while let Some(character) = chars.next() {
        match character {
            '{' if chars.as_str().starts_with('{') => return true,
            '{' => skip_placeholder(&mut chars),
            character if !character.is_whitespace() => return true,
            _ => {}
        }
    }
    false
}

// a fill char before an alignment can be `}`, as in `{name:}>8}`
fn skip_placeholder(chars: &mut Chars) {
    while let Some(character) = chars.next() {
        match character {
            '}' => return,
            ':' => {
                let mut spec = chars.clone();
                if spec.next().is_some() && matches!(spec.next(), Some('<' | '^' | '>')) {
                    *chars = spec;
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{has_placeholder, has_text_outside_placeholders};

    #[test]
    fn has_placeholder_takes_two_braces_as_a_brace() {
        let cases = [
            ("Is {name} tested?", true),
            ("{{{name}}}", true),
            ("Is `{{}}` a block?", false),
            ("Is it spam?", false),
        ];
        for (format, placeholder) in cases {
            assert_eq!(has_placeholder(format), placeholder, "{format:?}");
        }
    }

    #[test]
    fn has_text_outside_placeholders_skips_each_placeholder() {
        let cases = [
            ("Is {name} tested?", true),
            ("{name}?", true),
            ("{{}}", true),
            ("{{ {name}", true),
            ("{a} }}", true),
            ("", false),
            ("{name}", false),
            (" {a}\n{b:?} {c:>width$}\t", false),
            ("{name:}>0}", false),
            ("{name:}>8} a", true),
            ("{name:<<8}", false),
            ("{name:<<8}a", true),
            ("{name:}^0}", false),
        ];
        for (format, text) in cases {
            assert_eq!(has_text_outside_placeholders(format), text, "{format:?}");
        }
    }
}

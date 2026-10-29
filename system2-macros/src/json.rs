use crate::part::{has_placeholder, unescaped};
use proc_macro2::{Ident, Span, TokenStream};
use quote::{ToTokens, quote, quote_spanned};
use std::collections::HashSet;
use std::fmt::Write;
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::{Expr, ExprLit, Lit, LitFloat, LitInt, LitStr, Token, braced, bracketed, token};

// a json literal in a macro, such as `{ "question": "…", "rule": rule }`
pub(crate) enum Value {
    // the json text of a literal: text, a number, a bool, or null
    Literal(String),
    // text with a placeholder, as for `format!`
    Format(LitStr),
    Array(Vec<Value>),
    Object(Vec<(String, Value)>),
    Expression(Expr),
}

// the json of a literal, split at each expression from run time
#[derive(Default)]
struct Writer {
    // the json before each expression, and the expression
    interpolations: Vec<(String, TokenStream)>,
    // the json after the last expression
    pending_text: String,
}

impl Parse for Value {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        if input.peek(token::Brace) {
            let content;
            braced!(content in input);
            let mut keys = HashSet::new();
            let mut entries = Vec::new();
            while !content.is_empty() {
                let key: LitStr = content.parse().map_err(|error| {
                    syn::Error::new(
                        error.span(),
                        "a key of a JSON object must be a string literal",
                    )
                })?;
                if !keys.insert(key.value()) {
                    let message = format!("the key `{}` must occur one time at most", key.value());
                    return Err(syn::Error::new(key.span(), message));
                }
                content.parse::<Token![:]>()?;
                entries.push((key.value(), content.parse()?));
                if content.is_empty() {
                    break;
                }
                content.parse::<Token![,]>()?;
            }
            return Ok(Value::Object(entries));
        }
        if input.peek(token::Bracket) {
            let content;
            bracketed!(content in input);
            let items = Punctuated::<Value, Token![,]>::parse_terminated(&content)?;
            return Ok(Value::Array(items.into_iter().collect()));
        }

        // a minus before a number is part of the json, and before an expression is rust
        if input.peek(Token![-]) && (input.peek2(LitInt) || input.peek2(LitFloat)) {
            input.parse::<Token![-]>()?;
            return literal(&input.parse()?, "-");
        }
        match input.parse()? {
            Expr::Lit(ExprLit { lit, .. }) => literal(&lit, ""),
            Expr::Path(path) if path.path.is_ident("null") => Ok(Value::Literal("null".to_owned())),
            expression => Ok(Value::Expression(expression)),
        }
    }
}

impl Value {
    pub(crate) fn is_const(&self) -> bool {
        match self {
            Value::Literal(_) => true,
            Value::Format(_) | Value::Expression(_) => false,
            Value::Array(items) => items.iter().all(Value::is_const),
            Value::Object(entries) => entries.iter().all(|(_, value)| value.is_const()),
        }
    }

    fn write(&self, writer: &mut Writer) {
        match self {
            Value::Literal(json) => writer.pending_text.push_str(json),
            Value::Array(items) => {
                writer.pending_text.push('[');
                for (index, item) in items.iter().enumerate() {
                    if index > 0 {
                        writer.pending_text.push(',');
                    }
                    item.write(writer);
                }
                writer.pending_text.push(']');
            }
            Value::Object(entries) => {
                writer.pending_text.push('{');
                for (index, (key, value)) in entries.iter().enumerate() {
                    if index > 0 {
                        writer.pending_text.push(',');
                    }
                    writer.pending_text.push_str(&quoted(key));
                    writer.pending_text.push(':');
                    value.write(writer);
                }
                writer.pending_text.push('}');
            }
            Value::Format(literal) => {
                let prefix = std::mem::take(&mut writer.pending_text);
                let expression = quote_spanned!(literal.span()=> ::std::format!(#literal));
                writer.interpolations.push((prefix, expression));
            }
            Value::Expression(expression) => {
                let prefix = std::mem::take(&mut writer.pending_text);
                writer
                    .interpolations
                    .push((prefix, expression.to_token_stream()));
            }
        }
    }
}

impl ToTokens for Value {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let mut writer = Writer::default();
        self.write(&mut writer);
        let Writer {
            interpolations,
            pending_text,
        } = writer;

        let json = quote!(::system2::__private::Json);
        let cow = quote!(::std::borrow::Cow);
        if interpolations.is_empty() {
            tokens.extend(quote!(#json::Raw(#cow::Borrowed(#pending_text))));
            return;
        }

        // a const with the name of a binding matches it, so the name is one that no one writes
        let buffer = Ident::new("__system2_json", Span::mixed_site());

        // each value is a statement, so its temporaries, such as a `RefCell` borrow, drop before
        // the next value, and its span points a type error at the caller's value
        let pushes = interpolations.iter().map(|(prefix, expression)| {
            quote_spanned! {expression.span()=>
                #buffer.push_str(#prefix);
                #buffer.push_str(&::system2::__private::to_json(&(#expression)));
            }
        });
        tokens.extend(quote! {{
            let mut #buffer = ::std::string::String::new();
            #(#pushes)*
            #buffer.push_str(#pending_text);
            #json::Raw(#cow::Owned(#buffer))
        }});
    }
}

fn literal(lit: &Lit, sign: &str) -> syn::Result<Value> {
    let json = match lit {
        Lit::Str(text) if has_placeholder(&text.value()) => return Ok(Value::Format(text.clone())),

        // the macro writes this json text, so it unescapes each brace as `format!` does
        Lit::Str(text) => quoted(&unescaped(text)?),
        Lit::Bool(value) => value.value.to_string(),
        Lit::Int(number) if number.suffix().is_empty() => {
            format!("{sign}{}", number.base10_digits())
        }

        // rust takes `01.5` and `1.`, and json does not, so serde_json writes the float again
        Lit::Float(number) if number.suffix().is_empty() => {
            let Some(number) = serde_json::Number::from_f64(number.base10_parse()?) else {
                let message = "a JSON number must fit in an `f64`";
                return Err(syn::Error::new(lit.span(), message));
            };
            format!("{sign}{number}")
        }
        Lit::Int(_) | Lit::Float(_) => {
            let message = "a JSON number must have no suffix, such as `u32`";
            return Err(syn::Error::new(lit.span(), message));
        }
        _ => {
            let message = "a JSON value must be text, a number, `true`, `false`, `null`, \
                           an object, an array, or a variable";
            return Err(syn::Error::new(lit.span(), message));
        }
    };
    Ok(Value::Literal(json))
}

// the json text of a string
fn quoted(text: &str) -> String {
    let mut json = String::from('"');
    for character in text.chars() {
        match character {
            '"' => json.push_str("\\\""),
            '\\' => json.push_str("\\\\"),
            character if character < ' ' => {
                // a write to a string cannot fail
                _ = write!(json, "\\u{:04x}", u32::from(character));
            }
            character => json.push(character),
        }
    }
    json.push('"');
    json
}

#[cfg(test)]
mod tests {
    use super::{Value, Writer};
    use quote::{ToTokens, quote};

    fn written(tokens: proc_macro2::TokenStream) -> (Vec<String>, String) {
        let value: Value = syn::parse2(tokens).unwrap();
        let mut writer = Writer::default();
        value.write(&mut writer);
        let prefixes = writer
            .interpolations
            .iter()
            .map(|(prefix, _)| prefix.clone());
        (prefixes.collect(), writer.pending_text)
    }

    #[test]
    fn write_gives_the_json_text_of_each_literal() {
        let (prefixes, text) = written(quote! {
            [
                null, true, false, 7, -1, 0x10, 2.5, -1., 1e3, 01.5, 1e300, 1e-7,
                "a\"b\\c\nd\u{1}é", "{{}}", { "k\"{": [] }
            ]
        });

        assert_eq!(prefixes.len(), 0);
        assert_eq!(
            text,
            r#"[null,true,false,7,-1,16,2.5,-1.0,1000.0,1.5,1e+300,1e-7,"a\"b\\c\u000ad\u0001é","{}",{"k\"{":[]}]"#
        );
    }

    #[test]
    fn write_splits_the_text_at_each_expression() {
        let (prefixes, text) = written(quote!({ "a": x, "b": [-y, "{x}!"] }));

        assert_eq!(prefixes, [r#"{"a":"#, r#","b":["#, ","]);
        assert_eq!(text, "]}");
    }

    #[test]
    fn to_tokens_borrows_a_literal_with_no_expressions() {
        let value: Value = syn::parse2(quote!({ "a": 1 })).unwrap();

        assert_eq!(
            value.to_token_stream().to_string(),
            quote!(::system2::__private::Json::Raw(
                ::std::borrow::Cow::Borrowed("{\"a\":1}")
            ))
            .to_string()
        );
    }

    #[test]
    fn parse_refuses_a_bad_json_literal() {
        let cases = [
            (
                quote!({ question: "x" }),
                "a key of a JSON object must be a string literal",
            ),
            (
                quote!({ "a": 1, "a": 2 }),
                "the key `a` must occur one time at most",
            ),
            (
                quote!({ "a": 10u32 }),
                "a JSON number must have no suffix, such as `u32`",
            ),
            (
                quote!({ "a": -1.5f64 }),
                "a JSON number must have no suffix, such as `u32`",
            ),
            (
                quote!({ "a": 'x' }),
                "a JSON value must be text, a number, `true`, `false`, `null`, \
                 an object, an array, or a variable",
            ),
            (
                quote!({ "a": "a } b" }),
                "each `}` in text must be written two times, as `}}`",
            ),
            (quote!({ "a": 1e400 }), "a JSON number must fit in an `f64`"),
            (quote!({ "a" 1 }), "expected `:`"),
            (quote!({ "a": 1 "b": 2 }), "expected `,`"),
        ];
        for (tokens, message) in cases {
            let error = syn::parse2::<Value>(tokens.clone()).err().unwrap();
            assert_eq!(error.to_string(), message, "{tokens}");
        }
    }
}

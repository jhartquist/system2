use proc_macro2::{Span, TokenStream};
use quote::quote;
use std::collections::{HashMap, HashSet};
use syn::{Attribute, Data, DeriveInput, Expr, ExprLit, Fields, Ident, Lit, Meta, Variant};

struct Derive {
    trait_name: &'static str,
    question: &'static str,
    items: &'static str,
    an_item: &'static str,
    allows_rename: bool,
    max_variants: usize,
}

const OPTIONS: Derive = Derive {
    trait_name: "Options",
    question: "Choice",
    items: "options",
    an_item: "an option",
    allows_rename: true,
    max_variants: 255,
};

const LEVELS: Derive = Derive {
    trait_name: "Levels",
    question: "Score",
    items: "levels",
    an_item: "a level",
    allows_rename: false,
    max_variants: 10,
};

struct Entry<'a> {
    variant: &'a Ident,
    name: String,

    // the rename, or else the variant, so that an error about the name points at it
    span: Span,
    description: Option<String>,
}

pub(crate) fn options(input: &DeriveInput) -> syn::Result<TokenStream> {
    let entries = entries(input, &OPTIONS)?;

    let mut names = HashSet::new();
    let mut table = Vec::new();

    for entry in &entries {
        let Entry {
            variant,
            name,
            span,
            description,
        } = entry;
        if !names.insert(name) {
            let message = format!(
                r#"each option must have a different name, but two options have the name `{name}`: rename one with `#[system2(rename = "…")]`"#
            );
            return Err(syn::Error::new(*span, message));
        }

        // description if its not empty
        let description = match description {
            Some(description) if !description.trim().is_empty() => {
                quote!(::core::option::Option::Some(#description))
            }
            _ => quote!(::core::option::Option::None),
        };

        table.push(quote! {
            ::system2::ChoiceOption { name: #name, description: #description, value: || Self::#variant }
        });
    }

    let ident = &input.ident;
    let (impl_generics, type_generics, where_clause) = input.generics.split_for_impl();

    Ok(quote! {
        impl #impl_generics ::system2::Options for #ident #type_generics #where_clause {
            const OPTIONS: &'static [::system2::ChoiceOption<Self>] = &[#(#table),*];
        }
    })
}

pub(crate) fn levels(input: &DeriveInput) -> syn::Result<TokenStream> {
    let entries = entries(input, &LEVELS)?;

    // the names are the levels only when no variant has a description
    let described = entries.iter().any(|e| e.description.is_some());

    let mut texts = HashMap::new();
    let mut table = Vec::new();

    for entry in &entries {
        let Entry {
            variant,
            name,
            description,
            ..
        } = entry;
        let level = match description {
            Some(description) if description.trim().is_empty() => {
                let message = "the description of a level must not be empty";
                return Err(syn::Error::new(variant.span(), message));
            }
            Some(description) => description,
            // at least one level had a description, so they all should
            None if described => {
                let message = format!(
                    r#"the level `{variant}` must have a description, because other levels have one: add a doc comment or `#[system2(description = "…")]`"#
                );
                return Err(syn::Error::new(variant.span(), message));
            }
            None => name,
        };

        // ensure level is unique
        if let Some(first) = texts.insert(level, variant) {
            let source = if described { "description" } else { "name" };
            let message = format!(
                "each level must have a different {source}, but `{variant}` has the same {source} as `{first}`"
            );
            return Err(syn::Error::new(variant.span(), message));
        }

        table.push(quote!(::system2::ScoreLevel { level: #level, value: || Self::#variant }));
    }

    let ident = &input.ident;
    let (impl_generics, type_generics, where_clause) = input.generics.split_for_impl();

    Ok(quote! {
        impl #impl_generics ::system2::Levels for #ident #type_generics #where_clause {
            const LEVELS: &'static [::system2::ScoreLevel<Self>] = &[#(#table),*];
        }
    })
}

// validate the entire enum
fn entries<'a>(input: &'a DeriveInput, derive: &Derive) -> syn::Result<Vec<Entry<'a>>> {
    let Derive {
        trait_name,
        question,
        items,
        max_variants,
        ..
    } = derive;

    // ensure we are deriving on an enum
    let Data::Enum(data) = &input.data else {
        let message = format!(
            "`#[derive({trait_name})]` needs an enum: use `{question}::with_values` for other types"
        );
        return Err(syn::Error::new(input.ident.span(), message));
    };

    // only include #[system2(...)] attr on variants, not the full enum
    if let Some(attr) = input.attrs.iter().find(|a| a.path().is_ident("system2")) {
        let message = "`#[system2]` must be on a variant, not on the enum";
        return Err(syn::Error::new_spanned(attr, message));
    }

    // ensure we have a valid # of variants
    let count = data.variants.len();
    if count == 0 || count > *max_variants {
        // the first variant over the limit, or else the enum when it has none
        let span = match data.variants.iter().nth(*max_variants) {
            Some(extra) => extra.ident.span(),
            None => input.ident.span(),
        };
        let message = format!("a {question} must have 1 to {max_variants} {items}, not {count}");
        return Err(syn::Error::new(span, message));
    }

    // validate each variant and collect into Vec
    data.variants.iter().map(|v| entry(v, derive)).collect()
}

// validate each variant of the enum
fn entry<'a>(variant: &'a Variant, derive: &Derive) -> syn::Result<Entry<'a>> {
    let Derive {
        question,
        items,
        an_item,
        allows_rename,
        ..
    } = derive;
    let ident = &variant.ident;

    // only accept unit variants
    if !matches!(variant.fields, Fields::Unit) {
        let message = format!(
            "the variant `{ident}` must have no fields: use `{question}::with_values` for {items} that hold data"
        );
        return Err(syn::Error::new(ident.span(), message));
    }

    let mut rename = None;
    let mut description = None;
    for attr in variant
        .attrs
        .iter()
        .filter(|a| a.path().is_ident("system2"))
    {
        attr.parse_nested_meta(|meta| {
            // detect which slot to set
            let (key, slot) = match meta.path.get_ident() {
                Some(key) if key == "rename" && *allows_rename => (key, &mut rename),
                Some(key) if key == "description" => (key, &mut description),
                _ => {
                    let keys = if *allows_rename {
                        r#"`rename = "…"` and `description = "…"`"#
                    } else {
                        r#"`description = "…"`"#
                    };
                    let message = format!("{an_item} takes only {keys}");
                    return Err(meta.error(message));
                }
            };

            // prevent duplicate keys
            if slot.is_some() {
                return Err(meta.error(format!("`{key}` must occur one time at most")));
            }

            // only accept string values
            let value: Expr = meta.value()?.parse()?;
            let Expr::Lit(ExprLit {
                lit: Lit::Str(text),
                ..
            }) = value
            else {
                let message = format!("`{key}` must be a string literal");
                return Err(syn::Error::new_spanned(value, message));
            };

            // assign the slot
            *slot = Some(text);

            Ok(())
        })?;
    }

    // prevent `rename=""`
    if let Some(rename) = &rename
        && rename.value().trim().is_empty()
    {
        let message = "the name of an option must not be empty";
        return Err(syn::Error::new(rename.span(), message));
    }

    // use `rename` as the name, falling back to snake_case of original variant name
    let (name, span) = match rename {
        Some(rename) => (rename.value(), rename.span()),
        None => (snake_case(ident), ident.span()),
    };

    // use `description` as the description, falling back to doc comment
    let description = match description {
        Some(description) => Some(description.value()),
        None => doc_comment(&variant.attrs)?,
    };

    Ok(Entry {
        variant: ident,
        name,
        span,
        description,
    })
}

// as serde writes `rename_all = "snake_case"`, so the two derives agree
fn snake_case(ident: &Ident) -> String {
    let mut name = String::new();
    for (i, c) in ident.to_string().char_indices() {
        if i > 0 && c.is_uppercase() {
            name.push('_');
        }
        name.push(c.to_ascii_lowercase());
    }
    name
}

fn doc_comment(attrs: &[Attribute]) -> syn::Result<Option<String>> {
    let mut lines = Vec::new();
    for attr in attrs.iter().filter(|a| a.path().is_ident("doc")) {
        // only `#[doc = "…"]` holds text, not `#[doc(hidden)]` or `#[doc(alias = "…")]`
        let Meta::NameValue(meta) = &attr.meta else {
            continue;
        };
        let Expr::Lit(ExprLit {
            lit: Lit::Str(text),
            ..
        }) = &meta.value
        else {
            let message = r#"the doc comment must be a string literal: give the description with `#[system2(description = "…")]`"#;
            return Err(syn::Error::new_spanned(attr, message));
        };

        // not `lines()`, which gives no line for an empty `///`
        lines.extend(text.value().split('\n').map(|line| line.trim().to_owned()));
    }

    // an empty line ends a paragraph
    let paragraphs: Vec<_> = lines
        .split(String::is_empty)
        .filter(|paragraph| !paragraph.is_empty())
        .map(|paragraph| paragraph.join(" "))
        .collect();
    if paragraphs.is_empty() {
        return Ok(None);
    }

    Ok(Some(paragraphs.join("\n\n")))
}

#[cfg(test)]
mod tests {
    use super::{doc_comment, levels, options, snake_case};
    use proc_macro2::{Span, TokenStream};
    use quote::{format_ident, quote};
    use syn::{DeriveInput, Ident, Variant, parse_quote};

    fn message(result: syn::Result<TokenStream>) -> String {
        result.unwrap_err().to_string()
    }

    fn many(count: usize) -> DeriveInput {
        let variants = (1..=count).map(|i| format_ident!("V{i}"));
        syn::parse2(quote!(enum Many { #(#variants),* })).unwrap()
    }

    #[test]
    fn snake_case_writes_names_as_serde_does() {
        let cases = [
            ("Crash", "crash"),
            ("WrongResult", "wrong_result"),
            ("HTTPError", "h_t_t_p_error"),
            ("Level2A", "level2_a"),
        ];
        for (variant, name) in cases {
            assert_eq!(snake_case(&Ident::new(variant, Span::call_site())), name);
        }
    }

    #[test]
    fn doc_comment_joins_the_lines_as_rustdoc_shows_them() {
        let cases: [(Variant, Option<&str>); 6] = [
            (parse_quote!(Crash), None),
            (
                parse_quote! {
                    ///
                    #[doc(hidden)]
                    Crash
                },
                None,
            ),
            (
                parse_quote! {
                    #[doc(hidden)]
                    /// The code panics.
                    Crash
                },
                Some("The code panics."),
            ),
            (
                parse_quote! {
                    /// The code panics
                    /// for some input.
                    Crash
                },
                Some("The code panics for some input."),
            ),
            (
                parse_quote! {
                    /// The code panics.
                    ///
                    ///
                    /// It does not return.
                    Crash
                },
                Some("The code panics.\n\nIt does not return."),
            ),
            (
                parse_quote! {
                    /** The code panics
                    for some input. */
                    Crash
                },
                Some("The code panics for some input."),
            ),
        ];
        for (variant, text) in cases {
            assert_eq!(doc_comment(&variant.attrs).unwrap().as_deref(), text);
        }
    }

    #[test]
    fn options_writes_a_name_and_a_description_for_each_variant() {
        let tokens = options(&parse_quote! {
            enum Problem {
                /// The code panics.
                Crash,
                Style,
            }
        });

        let table = quote! {
            impl ::system2::Options for Problem {
                const OPTIONS: &'static [::system2::ChoiceOption<Self>] = &[
                    ::system2::ChoiceOption {
                        name: "crash",
                        description: ::core::option::Option::Some("The code panics."),
                        value: || Self::Crash
                    },
                    ::system2::ChoiceOption {
                        name: "style",
                        description: ::core::option::Option::None,
                        value: || Self::Style
                    }
                ];
            }
        };
        assert_eq!(tokens.unwrap().to_string(), table.to_string());
    }

    #[test]
    fn levels_writes_a_level_for_each_variant() {
        let tokens = levels(&parse_quote! {
            enum Severity {
                /// Nothing breaks.
                Minor,
                /// Data is lost.
                Critical,
            }
        });

        let table = quote! {
            impl ::system2::Levels for Severity {
                const LEVELS: &'static [::system2::ScoreLevel<Self>] = &[
                    ::system2::ScoreLevel { level: "Nothing breaks.", value: || Self::Minor },
                    ::system2::ScoreLevel { level: "Data is lost.", value: || Self::Critical }
                ];
            }
        };
        assert_eq!(tokens.unwrap().to_string(), table.to_string());
    }

    #[test]
    fn options_refuses_a_bad_enum() {
        let cases = [
            (
                quote! { struct Problem; },
                "`#[derive(Options)]` needs an enum: use `Choice::with_values` for other types",
            ),
            (
                quote! { enum Problem { Crash(u8) } },
                "the variant `Crash` must have no fields: use `Choice::with_values` for options that hold data",
            ),
            (
                quote! { enum Problem {} },
                "a Choice must have 1 to 255 options, not 0",
            ),
            (
                quote! { #[system2(rename = "problem")] enum Problem { Crash } },
                "`#[system2]` must be on a variant, not on the enum",
            ),
            (
                quote! { enum Problem { #[system2(name = "crash")] Crash } },
                r#"an option takes only `rename = "…"` and `description = "…"`"#,
            ),
            (
                quote! { enum Problem { #[system2(rename = "panic")] #[system2(rename = "crash")] Crash } },
                "`rename` must occur one time at most",
            ),
            (
                quote! { enum Problem { #[system2(rename)] Crash } },
                "expected `=`",
            ),
            (
                quote! { enum Problem { #[system2(rename = )] Crash } },
                "unexpected end of input, expected an expression",
            ),
            (
                quote! { enum Problem { #[system2(description = json!({ "what": "A panic." }))] Crash } },
                "`description` must be a string literal",
            ),
            (
                quote! { enum Problem { #[doc = include_str!("crash.md")] Crash } },
                r#"the doc comment must be a string literal: give the description with `#[system2(description = "…")]`"#,
            ),
            (
                quote! { enum Problem { #[system2(rename = " ")] Crash } },
                "the name of an option must not be empty",
            ),
            (
                quote! { enum Problem { HandOff, #[system2(rename = "hand_off")] Escalate } },
                r#"each option must have a different name, but two options have the name `hand_off`: rename one with `#[system2(rename = "…")]`"#,
            ),
        ];
        for (tokens, text) in cases {
            assert_eq!(message(options(&syn::parse2(tokens).unwrap())), text);
        }
        assert_eq!(
            message(options(&many(256))),
            "a Choice must have 1 to 255 options, not 256"
        );
    }

    #[test]
    fn options_takes_1_to_255_variants() {
        for count in [1, 255] {
            assert!(options(&many(count)).is_ok(), "{count}");
        }
    }

    #[test]
    fn levels_refuses_a_bad_enum() {
        let cases = [
            (
                quote! { struct Severity; },
                "`#[derive(Levels)]` needs an enum: use `Score::with_values` for other types",
            ),
            (
                quote! { enum Severity { Minor { count: u8 } } },
                "the variant `Minor` must have no fields: use `Score::with_values` for levels that hold data",
            ),
            (
                quote! { enum Severity {} },
                "a Score must have 1 to 10 levels, not 0",
            ),
            (
                quote! { enum Severity { #[system2(rename = "minor")] Minor } },
                r#"a level takes only `description = "…"`"#,
            ),
            (
                quote! { enum Severity { /** Nothing breaks. */ Minor, Major } },
                r#"the level `Major` must have a description, because other levels have one: add a doc comment or `#[system2(description = "…")]`"#,
            ),
            (
                quote! { enum Severity { /** Nothing breaks. */ #[system2(description = " ")] Minor } },
                "the description of a level must not be empty",
            ),
            (
                quote! { enum Severity { /** Bad. */ Minor, #[system2(description = "Bad.")] Major } },
                "each level must have a different description, but `Major` has the same description as `Minor`",
            ),
            (
                quote! { enum Severity { AB, A_b } },
                "each level must have a different name, but `A_b` has the same name as `AB`",
            ),
        ];
        for (tokens, text) in cases {
            assert_eq!(message(levels(&syn::parse2(tokens).unwrap())), text);
        }
        assert_eq!(
            message(levels(&many(11))),
            "a Score must have 1 to 10 levels, not 11"
        );
    }

    #[test]
    fn levels_takes_1_to_10_variants() {
        for count in [1, 10] {
            assert!(levels(&many(count)).is_ok(), "{count}");
        }
    }
}

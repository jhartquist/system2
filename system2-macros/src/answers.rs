use proc_macro2::{Span, TokenStream};
use quote::{quote, quote_spanned};
use syn::ext::IdentExt;
use syn::spanned::Spanned;
use syn::{Data, DataStruct, DeriveInput, Expr, Field, Fields, Ident, Index};

// `QuestionSet` has impls for tuples of 12 question sets at most
const MAX_TUPLE_LENGTH: usize = 12;

pub(crate) fn answers(input: &DeriveInput) -> syn::Result<TokenStream> {
    let ident = &input.ident;

    if let Some(attr) = input.attrs.iter().find(|a| a.path().is_ident("system2")) {
        let message = "`#[system2]` must be on a field, not on the struct";
        return Err(syn::Error::new_spanned(attr, message));
    }

    let Data::Struct(DataStruct {
        fields: Fields::Named(fields),
        ..
    }) = &input.data
    else {
        let message = "`#[derive(Answers)]` needs a struct with named fields";
        return Err(syn::Error::new(ident.span(), message));
    };

    if !input.generics.params.is_empty() {
        let message = "a named set must have no generic parameters, because its type alone \
                       selects its questions: use `questions! { … }` in a generic function";
        return Err(syn::Error::new_spanned(&input.generics, message));
    }

    if fields.named.is_empty() {
        let message = "a named set must have at least one field: \
                       add a field with `#[system2(question = …)]`";
        return Err(syn::Error::new(ident.span(), message));
    }

    let mut names = Vec::new();
    let mut answer_types = Vec::new();
    let mut question_sets = Vec::new();
    for field in &fields.named {
        #[expect(
            clippy::expect_used,
            reason = "a field of a struct with named fields has a name"
        )]
        let name = field.ident.as_ref().expect("a named field");
        let question = question(field, name)?;
        let answer_type = &field.ty;

        // at the question, so a question of another answer is an error there
        question_sets.push(quote_spanned! {question.span()=>
            ::system2::__private::question_set::<#answer_type, _>(const { #question })
        });
        names.push(name);
        answer_types.push(quote!(#answer_type));
    }

    let answer_set_type = nested_tuple(&answer_types);
    let question_set = nested_tuple(&question_sets);

    // a binding matches a constant of the same name, so a prefix makes that unlikely
    let answer_set = Ident::new("__answer_set", Span::call_site());
    let paths = (0..names.len()).map(|position| tuple_field_path(position, names.len()));

    Ok(quote! {
        impl ::system2::Answers for #ident {}

        impl ::system2::__private::SealedAnswers for #ident {
            type AnswerSet = #answer_set_type;

            fn question_set() -> impl ::system2::QuestionSet<AnswerSet = Self::AnswerSet> {
                #question_set
            }

            fn from_answer_set(#answer_set: Self::AnswerSet) -> Self {
                Self {
                    #(#names: #answer_set #paths,)*
                }
            }
        }
    })
}

fn question(field: &Field, name: &Ident) -> syn::Result<Expr> {
    let mut question = None;
    for attr in field.attrs.iter().filter(|a| a.path().is_ident("system2")) {
        attr.parse_nested_meta(|meta| {
            if !meta.path.is_ident("question") {
                return Err(meta.error("a field takes only `question = …`"));
            }
            if question.is_some() {
                return Err(meta.error("`question` must occur one time at most"));
            }

            // a `const` cannot use `?`, and rustc tells only that it cannot return there
            let value: Expr = meta.value()?.parse()?;
            if let Expr::Try(_) = value {
                let message = "a question of a named set must be a constant: \
                               use `questions! { … }` for questions from data";
                return Err(syn::Error::new_spanned(value, message));
            }

            question = Some(value);
            Ok(())
        })?;
    }

    question.ok_or_else(|| {
        let message = format!(
            "the field `{}` must name its question with `#[system2(question = …)]`",
            name.unraw()
        );
        syn::Error::new(name.span(), message)
    })
}

// past 12 items, the last item of a tuple is a tuple of the rest
fn nested_tuple(items: &[TokenStream]) -> TokenStream {
    if items.len() <= MAX_TUPLE_LENGTH {
        return quote!((#(#items,)*));
    }
    let (head, rest) = items.split_at(MAX_TUPLE_LENGTH - 1);
    let rest = nested_tuple(rest);
    quote!((#(#head,)* #rest,))
}

// the path to an item of `nested_tuple`, as `.11.0`
fn tuple_field_path(position: usize, count: usize) -> TokenStream {
    let last = MAX_TUPLE_LENGTH - 1;
    if count <= MAX_TUPLE_LENGTH || position < last {
        let index = Index::from(position);
        return quote!(.#index);
    }
    let index = Index::from(last);
    let rest = tuple_field_path(position - last, count - last);
    quote!(.#index #rest)
}

#[cfg(test)]
mod tests {
    use super::{answers, nested_tuple, tuple_field_path};
    use proc_macro2::TokenStream;
    use quote::quote;

    fn message(tokens: TokenStream) -> String {
        answers(&syn::parse2(tokens).unwrap())
            .unwrap_err()
            .to_string()
    }

    fn items(count: usize) -> Vec<TokenStream> {
        (0..count).map(|index| quote!(#index)).collect()
    }

    #[test]
    fn answers_writes_a_tuple_of_the_fields() {
        let tokens = answers(&syn::parse_quote! {
            struct Review {
                /// Whether the code has a bug.
                #[system2(question = BUGGY)]
                buggy: NoulAnswer,
                #[system2(question = noul!("Is it tested?"))]
                r#tested: NoulAnswer,
            }
        });

        let private = quote!(::system2::__private);
        let implementation = quote! {
            impl ::system2::Answers for Review {}

            impl #private::SealedAnswers for Review {
                type AnswerSet = (NoulAnswer, NoulAnswer,);

                fn question_set() -> impl ::system2::QuestionSet<AnswerSet = Self::AnswerSet> {
                    (
                        #private::question_set::<NoulAnswer, _>(const { BUGGY }),
                        #private::question_set::<NoulAnswer, _>(const { noul!("Is it tested?") }),
                    )
                }

                fn from_answer_set(__answer_set: Self::AnswerSet) -> Self {
                    Self {
                        buggy: __answer_set.0,
                        r#tested: __answer_set.1,
                    }
                }
            }
        };
        assert_eq!(tokens.unwrap().to_string(), implementation.to_string());
    }

    #[test]
    fn nested_tuple_holds_the_items_past_12_in_its_last_item() {
        let cases = [
            (1, quote!((0usize,))),
            (
                12,
                quote!((
                    0usize, 1usize, 2usize, 3usize, 4usize, 5usize, 6usize, 7usize, 8usize, 9usize,
                    10usize, 11usize,
                )),
            ),
            (
                13,
                quote!((
                    0usize,
                    1usize,
                    2usize,
                    3usize,
                    4usize,
                    5usize,
                    6usize,
                    7usize,
                    8usize,
                    9usize,
                    10usize,
                    (11usize, 12usize,),
                )),
            ),
        ];
        for (count, tokens) in cases {
            assert_eq!(
                nested_tuple(&items(count)).to_string(),
                tokens.to_string(),
                "{count}"
            );
        }
        let nested_two_times = nested_tuple(&items(24)).to_string();
        assert!(nested_two_times.ends_with("21usize , (22usize , 23usize ,) ,) ,)"));
    }

    #[test]
    fn tuple_field_path_finds_each_item_of_nested_tuple() {
        let cases = [
            (0, 1, ". 0"),
            (11, 12, ". 11"),
            (10, 13, ". 10"),
            (11, 13, ". 11 . 0"),
            (12, 13, ". 11 . 1"),
            (23, 24, ". 11 . 11 . 1"),
        ];
        for (position, count, text) in cases {
            assert_eq!(tuple_field_path(position, count).to_string(), text);
        }
    }

    #[test]
    fn answers_refuses_a_bad_struct() {
        let cases = [
            (
                quote! { enum Review { Buggy } },
                "`#[derive(Answers)]` needs a struct with named fields",
            ),
            (
                quote! { struct Review(NoulAnswer); },
                "`#[derive(Answers)]` needs a struct with named fields",
            ),
            (
                quote! { struct Review; },
                "`#[derive(Answers)]` needs a struct with named fields",
            ),
            (
                quote! { struct Review<T> { #[system2(question = BUGGY)] buggy: T } },
                "a named set must have no generic parameters, because its type alone selects \
                 its questions: use `questions! { … }` in a generic function",
            ),
            (
                quote! { struct Review<'a> { #[system2(question = BUGGY)] buggy: &'a str } },
                "a named set must have no generic parameters, because its type alone selects \
                 its questions: use `questions! { … }` in a generic function",
            ),
            (
                quote! { struct Review {} },
                "a named set must have at least one field: \
                 add a field with `#[system2(question = …)]`",
            ),
            (
                quote! {
                    #[system2(question = BUGGY)]
                    struct Review { #[system2(question = BUGGY)] buggy: NoulAnswer }
                },
                "`#[system2]` must be on a field, not on the struct",
            ),
            (
                quote! { struct Review { buggy: NoulAnswer } },
                "the field `buggy` must name its question with `#[system2(question = …)]`",
            ),
            (
                quote! { struct Review { r#type: NoulAnswer } },
                "the field `type` must name its question with `#[system2(question = …)]`",
            ),
            (
                quote! { struct Review { #[system2(description = "x")] buggy: NoulAnswer } },
                "a field takes only `question = …`",
            ),
            (
                quote! {
                    struct Review {
                        #[system2(question = BUGGY)]
                        #[system2(question = TESTED)]
                        buggy: NoulAnswer,
                    }
                },
                "`question` must occur one time at most",
            ),
            (
                quote! { struct Review { #[system2(question)] buggy: NoulAnswer } },
                "expected `=`",
            ),
            (
                quote! { struct Review { #[system2(question = Noul::new(&text)?)] rule: NoulAnswer } },
                "a question of a named set must be a constant: \
                 use `questions! { … }` for questions from data",
            ),
        ];
        for (tokens, text) in cases {
            assert_eq!(message(tokens.clone()), text, "{tokens}");
        }
    }
}

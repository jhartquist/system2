use proc_macro2::{Span, TokenStream};
use quote::{ToTokens, format_ident, quote, quote_spanned};
use std::collections::HashSet;
use syn::ext::IdentExt;
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::{Expr, Ident, Token};

pub(crate) struct Questions {
    fields: Vec<Field>,
}

struct Field {
    name: Ident,
    value: Option<Expr>,
}

impl Parse for Questions {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let fields = Punctuated::<Field, Token![,]>::parse_terminated(input)?;
        let mut names = HashSet::new();
        for field in &fields {
            let name = field.name.unraw();
            if !names.insert(name.clone()) {
                let message = format!("the field `{name}` must occur one time at most");
                return Err(syn::Error::new(field.name.span(), message));
            }
        }
        let fields = fields.into_iter().collect();
        Ok(Questions { fields })
    }
}

impl Parse for Field {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let name = input.parse()?;
        let value = match input.parse::<Option<Token![:]>>()? {
            Some(_) => Some(input.parse()?),
            None => None,
        };
        Ok(Field { name, value })
    }
}

impl Field {
    fn value(&self) -> TokenStream {
        match &self.value {
            Some(value) => value.to_token_stream(),
            None => self.name.to_token_stream(),
        }
    }
}

impl ToTokens for Questions {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let private = quote!(::system2::__private);
        let names: Vec<&Ident> = self.fields.iter().map(|field| &field.name).collect();
        let types: Vec<Ident> = (0..names.len())
            .map(|index| format_ident!("F{index}"))
            .collect();
        let indexes: Vec<syn::Index> = (0..names.len()).map(syn::Index::from).collect();

        // a binding in a pattern matches a constant of the same name, and hygiene cannot stop
        // that, so a prefix only makes it unlikely; mixed site hides each binding from the caller
        let raw_question_set = Ident::new("__raw_question_set", Span::mixed_site());
        let raw_answer_set = Ident::new("__raw_answer_set", Span::mixed_site());

        // the type of each value selects its kind, and a wrong kind fails at the field
        let values = self.fields.iter().map(|field| {
            let value = field.value();

            // the span of the value, so an error is at the field; no code of the caller sees
            // the binding, so it needs no mixed site
            let field_binding = Ident::new("__field", value.span());
            quote_spanned! {value.span()=>
                #private::field(#value, |#field_binding| {
                    #[allow(unused_imports)]
                    use #private::{
                        IteratorKind as _, QuestionSetKind as _, ReferenceKind as _,
                        ResultKind as _, ValueKind as _,
                    };
                    (&&&&&&#private::Probe::of(&#field_binding))
                        .field_kind()
                        .into_field(#field_binding)
                })
            }
        });

        // one call with the values as its arguments, so a temporary of a value lives to the end
        // of the statement of the caller, and the structs in the block hide no name of the caller
        tokens.extend(quote! {
            ({
                #[derive(::core::fmt::Debug, ::core::clone::Clone)]
                struct Questions<#(#types),*>(#(#types),*);

                #[derive(::core::fmt::Debug, ::core::clone::Clone, #private::serde::Serialize)]
                #[serde(crate = "::system2::__private::serde")]
                struct Answers<#(#types),*> {
                    #(#names: #types),*
                }

                impl<#(#types: #private::Field),*> ::system2::QuestionSet
                    for Questions<#(#types),*>
                {
                    type AnswerSet = Answers<#(<#types as #private::Field>::AnswerSet),*>;
                }

                impl<#(#types: #private::Field),*> #private::Sealed for Questions<#(#types),*> {
                    fn write(&mut self, #raw_question_set: &mut #private::RawQuestionSet) {
                        #(#private::Field::write(&mut self.#indexes, #raw_question_set);)*
                    }

                    fn read(
                        self,
                        #raw_answer_set: &mut #private::RawAnswerSet,
                    ) -> ::core::result::Result<
                        <Self as ::system2::QuestionSet>::AnswerSet,
                        ::std::string::String,
                    > {
                        ::core::result::Result::Ok(Answers {
                            #(#names: #private::Field::read(self.#indexes, #raw_answer_set)?,)*
                        })
                    }
                }

                Questions
            })(#(#values),*)
        });
    }
}

#[cfg(test)]
mod tests {
    use super::Questions;
    use quote::{ToTokens, quote};
    use syn::{ExprBlock, ExprCall, ExprParen, Item, Stmt};

    #[test]
    fn parse_takes_a_field_with_only_a_name_and_a_trailing_comma() {
        let questions: Questions = syn::parse2(quote!(buggy: BUGGY, checks,)).unwrap();

        let values: Vec<String> = questions
            .fields
            .iter()
            .map(|field| field.value().to_string())
            .collect();
        assert_eq!(values, ["BUGGY", "checks"]);
    }

    #[test]
    fn parse_refuses_bad_input() {
        let cases = [
            (
                quote!(buggy: BUGGY, tested: TESTED, buggy: TESTED),
                "the field `buggy` must occur one time at most",
            ),
            (
                quote!(id: 1, r#id: 2),
                "the field `id` must occur one time at most",
            ),
            (quote!(buggy: BUGGY tested: TESTED), "expected `,`"),
            (quote!("buggy": BUGGY), "expected identifier"),
        ];
        for (tokens, message) in cases {
            let error = syn::parse2::<Questions>(tokens.clone()).err().unwrap();
            assert_eq!(error.to_string(), message, "{tokens}");
        }
    }

    #[test]
    fn to_tokens_gives_a_call_with_a_struct_of_questions_and_of_answers() {
        let questions: Questions = syn::parse2(quote!(buggy: BUGGY, id, tested: TESTED)).unwrap();

        let call: ExprCall = syn::parse2(questions.to_token_stream()).unwrap();
        let callee: ExprParen = syn::parse2(call.func.to_token_stream()).unwrap();
        let callee: ExprBlock = syn::parse2(callee.expr.to_token_stream()).unwrap();
        let structs: Vec<(String, String, Vec<String>)> = callee
            .block
            .stmts
            .iter()
            .filter_map(|stmt| match stmt {
                Stmt::Item(Item::Struct(item)) => Some(item),
                _ => None,
            })
            .map(|item| {
                let generics = item.generics.to_token_stream().to_string();
                let fields = item
                    .fields
                    .iter()
                    .map(|field| field.to_token_stream().to_string());
                (item.ident.to_string(), generics, fields.collect())
            })
            .collect();
        let generics = "< F0 , F1 , F2 >".to_owned();
        let types = ["F0", "F1", "F2"].map(str::to_owned).to_vec();
        let fields = ["buggy : F0", "id : F1", "tested : F2"]
            .map(str::to_owned)
            .to_vec();
        assert_eq!(
            structs,
            [
                ("Questions".to_owned(), generics.clone(), types),
                ("Answers".to_owned(), generics, fields),
            ]
        );
        assert_eq!(call.args.len(), 3);
    }
}

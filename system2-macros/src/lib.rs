//! The macros of `system2`.

mod choice;
mod derive;
mod json;
mod noul;
mod part;
mod score;

use proc_macro::TokenStream;
use quote::ToTokens;
use syn::{DeriveInput, parse_macro_input};

#[proc_macro]
pub fn noul(input: TokenStream) -> TokenStream {
    let noul = parse_macro_input!(input as noul::Noul);
    noul.into_token_stream().into()
}

#[proc_macro]
pub fn choice(input: TokenStream) -> TokenStream {
    let choice = parse_macro_input!(input as choice::Choice);
    choice.into_token_stream().into()
}

#[proc_macro]
pub fn score(input: TokenStream) -> TokenStream {
    let score = parse_macro_input!(input as score::Score);
    score.into_token_stream().into()
}

#[proc_macro_derive(Options, attributes(system2))]
pub fn derive_options(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    derive::options(&input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

#[proc_macro_derive(Levels, attributes(system2))]
pub fn derive_levels(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    derive::levels(&input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

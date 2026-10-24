//! The macros of `system2`.

mod derive;

use proc_macro::TokenStream;
use syn::{DeriveInput, parse_macro_input};

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

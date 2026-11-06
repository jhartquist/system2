use proc_macro2::TokenStream;
use quote::quote;
use syn::LitStr;
#[cfg(feature = "image")]
use {
    proc_macro2::Literal,
    std::fs,
    system2_shared::image::{Converted, Header},
};

// reads the file as `include_bytes!` does, from the folder of the file of the call
#[cfg(feature = "image")]
pub(crate) fn include_image(path: &LitStr) -> TokenStream {
    let file = path.span().unwrap().local_file();
    let bytes = file.and_then(|file| fs::read(file.with_file_name(path.value())).ok());
    expansion(path, bytes.as_deref())
}

// with no feature, the file must pass as it is
#[cfg(not(feature = "image"))]
pub(crate) fn include_image(path: &LitStr) -> TokenStream {
    file_image(path)
}

// `image!` of the bytes validates the file when the crate builds
fn file_image(path: &LitStr) -> TokenStream {
    quote!(::system2::image!(::core::include_bytes!(#path)))
}

// rust-analyzer gives no file, and a file that cannot be read gets the error of `include_bytes!`.
// a converted image keeps the file as its original
#[cfg(feature = "image")]
fn expansion(path: &LitStr, bytes: Option<&[u8]>) -> TokenStream {
    let Some(bytes) = bytes else {
        return file_image(path);
    };
    match system2_shared::image::convert(bytes) {
        Ok(None) => file_image(path),
        Ok(Some(Converted {
            bytes,
            original: Header { width, height },
        })) => {
            let converted = Literal::byte_string(&bytes);
            quote! {
                ::system2::__private::converted_image(
                    #converted,
                    ::core::include_bytes!(#path),
                    #width,
                    #height,
                )
            }
        }
        Err(error) => syn::Error::new(path.span(), error).into_compile_error(),
    }
}

#[cfg(all(test, feature = "image"))]
mod tests {
    use super::expansion;
    use quote::quote;
    use syn::{LitStr, parse_quote};

    // a png header of 300x300 px, which passes as it is
    const PNG: &[u8] =
        b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR\0\0\x01\x2c\0\0\x01\x2c\x08\x02\0\0\0\0\0\0\0";

    // a gif of 1x1 px, which becomes a png of 280x280 px
    const GIF: &[u8] =
        b"GIF89a\x01\0\x01\0\x80\0\0\xff\xff\xff\0\0\0,\0\0\0\0\x01\0\x01\0\0\x02\x02D\x01\0;";

    #[test]
    fn expansion_converts_only_an_image_that_needs_it() {
        let path: LitStr = parse_quote!("logo.gif");
        let file_image = quote!(::system2::image!(::core::include_bytes!("logo.gif"))).to_string();

        let unread = expansion(&path, None).to_string();
        let passed = expansion(&path, Some(PNG)).to_string();
        let converted = expansion(&path, Some(GIF)).to_string();
        let refused = expansion(&path, Some(b"not an image")).to_string();

        assert_eq!(unread, file_image);
        assert_eq!(passed, file_image);
        assert!(converted.starts_with(":: system2 :: __private :: converted_image (b\"\\x89PNG"));
        assert!(
            converted.ends_with(", :: core :: include_bytes ! (\"logo.gif\") , 1u32 , 1u32 ,)")
        );
        assert!(refused.contains("the image must be a valid PNG, JPEG, WebP, or GIF"));
    }
}

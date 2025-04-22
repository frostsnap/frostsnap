use proc_macro::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Fields, parse_macro_input};

#[proc_macro_derive(Kind, attributes(delegate_kind))]
pub fn derive_kind(input: TokenStream) -> TokenStream {
    // Parse the input tokens into a syntax tree
    let input = parse_macro_input!(input as DeriveInput);
    let name = input.ident;

    // Only allow enums
    let data_enum = match input.data {
        Data::Enum(data_enum) => data_enum,
        _ => {
            return syn::Error::new_spanned(name, "Kind can only be derived for enums")
                .to_compile_error()
                .into();
        }
    };

    // Build a match arm for each variant of the enum.
    let match_arms = data_enum.variants.into_iter().map(|variant| {
        let variant_ident = variant.ident;
        // Check if the variant has the attribute `#[delegate_kind]`
        let has_delegate = variant.attrs.iter().any(|attr| attr.path.is_ident("delegate_kind"));

        if has_delegate {
            // Ensure it's a newtype variant (tuple variant with exactly one field)
            match variant.fields {
                Fields::Unnamed(ref fields) if fields.unnamed.len() == 1 => {
                    // Delegate the `kind` call to the inner type
                    quote! {
                        #name::#variant_ident(inner) => inner.kind(),
                    }
                },
                _ => {
                    // Generate an error if the attribute is applied to a variant that is not a newtype
                    return syn::Error::new_spanned(
                        variant_ident,
                        "delegate_kind attribute can only be applied to newtype variants (tuple struct with exactly one field)"
                    )
                    .to_compile_error();
                }
            }
        } else {
            // Without the attribute, simply return the variant name as a string literal.
            let variant_name = variant_ident.to_string();
            match variant.fields {
                Fields::Unit => quote! {
                    #name::#variant_ident => #variant_name,
                },
                Fields::Unnamed(_) => quote! {
                    #name::#variant_ident(..) => #variant_name,
                },
                Fields::Named(_) => quote! {
                    #name::#variant_ident { .. } => #variant_name,
                },
            }
        }
    });

    // Generate the final implementation of the `Kind` trait for the enum
    let expanded = quote! {
        impl Kind for #name {
            fn kind(&self) -> &'static str {
                match self {
                    #(#match_arms)*
                }
            }
        }
    };

    TokenStream::from(expanded)
}

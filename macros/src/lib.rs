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
                    syn::Error::new_spanned(
                        variant_ident,
                        "delegate_kind attribute can only be applied to newtype variants (tuple struct with exactly one field)"
                    )
                    .to_compile_error()
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

#[proc_macro_derive(Widget)]
pub fn derive_widget(input: TokenStream) -> TokenStream {
    // Parse the input tokens into a syntax tree
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;
    let generics = &input.generics;
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    // Only allow enums
    let data_enum = match &input.data {
        Data::Enum(data_enum) => data_enum,
        _ => {
            return syn::Error::new_spanned(name, "Widget can only be derived for enums")
                .to_compile_error()
                .into();
        }
    };

    // Generate match arms for each method
    let draw_arms = generate_match_arms(&data_enum.variants, quote!(draw(target, current_time)));
    let handle_touch_arms = generate_match_arms(&data_enum.variants, quote!(handle_touch(point, current_time, is_release)));
    let handle_vertical_drag_arms = generate_match_arms(&data_enum.variants, quote!(handle_vertical_drag(prev_y, new_y, is_release)));
    let size_hint_arms = generate_match_arms(&data_enum.variants, quote!(size_hint()));
    let force_full_redraw_arms = generate_match_arms(&data_enum.variants, quote!(force_full_redraw()));

    // Generate both DynWidget and Widget trait implementations
    let expanded = quote! {
        impl #impl_generics frostsnap_embedded_widgets::DynWidget for #name #ty_generics #where_clause {
            fn handle_touch(
                &mut self,
                point: embedded_graphics::geometry::Point,
                current_time: frostsnap_embedded_widgets::Instant,
                is_release: bool,
            ) -> Option<frostsnap_embedded_widgets::KeyTouch> {
                match self {
                    #(#handle_touch_arms)*
                }
            }

            fn handle_vertical_drag(&mut self, prev_y: Option<u32>, new_y: u32, is_release: bool) {
                match self {
                    #(#handle_vertical_drag_arms)*
                }
            }

            fn size_hint(&self) -> Option<embedded_graphics::geometry::Size> {
                match self {
                    #(#size_hint_arms)*
                }
            }

            fn force_full_redraw(&mut self) {
                match self {
                    #(#force_full_redraw_arms)*
                }
            }
        }

        impl #impl_generics frostsnap_embedded_widgets::Widget for #name #ty_generics #where_clause {
            type Color = embedded_graphics::pixelcolor::Rgb565;

            fn draw<D: embedded_graphics::draw_target::DrawTarget<Color = Self::Color>>(
                &mut self,
                target: &mut D,
                current_time: frostsnap_embedded_widgets::Instant,
            ) -> Result<(), D::Error> {
                match self {
                    #(#draw_arms)*
                }
            }
        }
    };

    TokenStream::from(expanded)
}

fn generate_match_arms(
    variants: &syn::punctuated::Punctuated<syn::Variant, syn::token::Comma>,
    method_call: proc_macro2::TokenStream,
) -> Vec<proc_macro2::TokenStream> {
    variants
        .iter()
        .map(|variant| {
            let variant_ident = &variant.ident;
            match &variant.fields {
                Fields::Unit => {
                    // For unit variants, we can't delegate, so panic or return default
                    quote! {
                        Self::#variant_ident => panic!("Unit variant {} cannot delegate Widget methods", stringify!(#variant_ident)),
                    }
                }
                Fields::Unnamed(fields) => {
                    if fields.unnamed.len() == 1 {
                        // Single field tuple variant - delegate to inner widget
                        quote! {
                            Self::#variant_ident(widget) => widget.#method_call,
                        }
                    } else {
                        // Multiple fields - assume first field is the widget
                        quote! {
                            Self::#variant_ident(widget, ..) => widget.#method_call,
                        }
                    }
                }
                Fields::Named(_) => {
                    // For named fields, check if there's a field named 'widget'
                    quote! {
                        Self::#variant_ident { widget, .. } => widget.#method_call,
                    }
                }
            }
        })
        .collect()
}

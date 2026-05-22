use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{ToTokens, format_ident, quote};
use syn::{
    Data, DeriveInput, Fields, GenericArgument, ItemStruct, PathArguments, Type, parse::Parse,
    parse::ParseStream, parse_macro_input,
};

/// Emits a non-opaque "leaf handle" type for a Rust-managed broadcast
/// stream, plus its FRB shim and Dart-side `Stream<T> watch()`.
///
/// Input is a single tuple struct: `pub struct Name(pub Inner<T>);` where
/// `Inner` is either `Broadcast` or `BehaviorBroadcast`. The field type is
/// wrapped in `RustAutoOpaque<...>` and `#[frb(non_opaque)]` is attached.
///
/// Requirements at the call site:
/// - `use flutter_rust_bridge::frb;` so `#[frb(...)]` attributes resolve.
///
/// All other paths in the emitted code are fully qualified.
#[proc_macro]
pub fn broadcast_handle(input: TokenStream) -> TokenStream {
    let spec = parse_macro_input!(input as BroadcastHandleSpec);
    expand_broadcast_handle(spec)
        .unwrap_or_else(|err| err.to_compile_error())
        .into()
}

struct BroadcastHandleSpec {
    item_struct: ItemStruct,
}

impl Parse for BroadcastHandleSpec {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let item_struct = input.parse()?;
        if !input.is_empty() {
            return Err(input.error(
                "broadcast_handle! expects exactly one struct: `pub struct Name(pub Inner<T>);`",
            ));
        }
        Ok(Self { item_struct })
    }
}

fn expand_broadcast_handle(spec: BroadcastHandleSpec) -> syn::Result<TokenStream2> {
    let mut item_struct = spec.item_struct;

    for attr in &item_struct.attrs {
        if attr.path.is_ident("frb") {
            return Err(syn::Error::new_spanned(
                attr,
                "broadcast_handle! does not accept #[frb(...)] attributes; the macro emits non_opaque + dart_code itself",
            ));
        }
    }

    if !matches!(item_struct.vis, syn::Visibility::Public(_)) {
        return Err(syn::Error::new_spanned(
            &item_struct.ident,
            "broadcast_handle! struct must be `pub`",
        ));
    }

    let struct_ident = item_struct.ident.clone();

    let field = match &mut item_struct.fields {
        Fields::Unnamed(fields) if fields.unnamed.len() == 1 => &mut fields.unnamed[0],
        _ => {
            return Err(syn::Error::new_spanned(
                &item_struct.fields,
                "broadcast_handle! requires a tuple struct with exactly one field: `pub struct Name(pub Inner<T>);`",
            ));
        }
    };

    if !matches!(field.vis, syn::Visibility::Public(_)) {
        return Err(syn::Error::new_spanned(
            &field.ty,
            "broadcast_handle! field must be `pub`",
        ));
    }

    let inner_ty = field.ty.clone();
    let element_ty = element_type_from_inner(&inner_ty)?;
    field.ty = syn::parse_quote!(crate::frb_generated::RustAutoOpaque<#inner_ty>);

    let req_ident = format_ident!("{}WatchReq", struct_ident);
    let dart_type_str = dart_type(&element_ty);
    let dart_code = build_handle_dart_code(&req_ident, &dart_type_str);
    let dart_lit = syn::LitStr::new(&dart_code, proc_macro2::Span::call_site());

    item_struct
        .attrs
        .push(syn::parse_quote!(#[frb(non_opaque)]));
    item_struct
        .attrs
        .push(syn::parse_quote!(#[frb(dart_code = #dart_lit)]));

    let req_struct = quote! {
        pub struct #req_ident {
            pub sink: crate::frb_generated::StreamSink<#element_ty>,
        }
    };

    let impl_block = quote! {
        impl #struct_ident {
            #[frb(ignore)]
            pub fn new(inner: #inner_ty) -> Self {
                Self(crate::frb_generated::RustAutoOpaque::new(inner))
            }

            #[frb(sync)]
            pub fn subscriber_count(&self) -> u32 {
                self.0.blocking_read().subscriber_count()
            }

            #[frb(sync)]
            pub fn detach(&self, id: crate::api::broadcast::SinkRegistrationId) -> bool {
                self.0.blocking_read().unregister(id)
            }

            #[frb(sync)]
            pub fn frb_attach_watch(
                &self,
                req: #req_ident,
            ) -> crate::api::broadcast::SinkRegistrationId {
                let #req_ident { sink } = req;
                self.0.blocking_read().register(sink)
            }
        }
    };

    Ok(quote! {
        #req_struct
        #item_struct
        #impl_block
    })
}

fn element_type_from_inner(ty: &Type) -> syn::Result<Type> {
    let Type::Path(path) = ty else {
        return Err(syn::Error::new_spanned(
            ty,
            "broadcast_handle! field type must be `Broadcast<T>` or `BehaviorBroadcast<T>`",
        ));
    };
    let Some(last) = path.path.segments.last() else {
        return Err(syn::Error::new_spanned(
            ty,
            "broadcast_handle! field type must be `Broadcast<T>` or `BehaviorBroadcast<T>`",
        ));
    };
    let last_ident = last.ident.to_string();
    if last_ident != "Broadcast" && last_ident != "BehaviorBroadcast" {
        return Err(syn::Error::new_spanned(
            &last.ident,
            "broadcast_handle! field type must be `Broadcast<T>` or `BehaviorBroadcast<T>`",
        ));
    }
    let PathArguments::AngleBracketed(args) = &last.arguments else {
        return Err(syn::Error::new_spanned(
            &last.arguments,
            "broadcast_handle! field type must have one generic arg, e.g. Broadcast<()>",
        ));
    };
    let Some(GenericArgument::Type(item_ty)) = args.args.first() else {
        return Err(syn::Error::new_spanned(
            &args.args,
            "broadcast_handle! field type's first generic arg must be a type",
        ));
    };
    Ok(item_ty.clone())
}

fn build_handle_dart_code(req_ident: &syn::Ident, dart_type: &str) -> String {
    format!(
        "\n  Stream<{dart_type}> watch() =>\n      rustBroadcastStream<{dart_type}>(\n        attach: (sink) => frbAttachWatch(req: {req_ident}(sink: sink)),\n        detach: (id) => detach(id: id as SinkRegistrationId),\n      );\n",
    )
}

fn dart_type(ty: &Type) -> String {
    match ty {
        Type::Tuple(tuple) if tuple.elems.is_empty() => "void".to_string(),
        Type::Path(path) => {
            let Some(last) = path.path.segments.last() else {
                return ty.to_token_stream().to_string();
            };
            match last.ident.to_string().as_str() {
                "i32" | "i64" | "u32" | "u64" | "usize" => "int".to_string(),
                "f64" | "f32" => "double".to_string(),
                "String" => "String".to_string(),
                "bool" => "bool".to_string(),
                other => other.to_string(),
            }
        }
        _ => ty.to_token_stream().to_string(),
    }
}

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

#[proc_macro_derive(Widget, attributes(widget_delegate, widget_crate))]
pub fn derive_widget(input: TokenStream) -> TokenStream {
    // Parse the input tokens into a syntax tree
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;
    let generics = &input.generics;
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    // Determine the crate path to use
    let crate_path = get_crate_path(&input.attrs);

    match &input.data {
        Data::Enum(data_enum) => derive_widget_for_enum(
            name,
            &impl_generics,
            &ty_generics,
            &where_clause,
            data_enum,
            &crate_path,
        ),
        Data::Struct(data_struct) => derive_widget_for_struct(
            name,
            &impl_generics,
            &ty_generics,
            &where_clause,
            data_struct,
            &input.attrs,
            &crate_path,
        ),
        _ => syn::Error::new_spanned(name, "Widget can only be derived for enums and structs")
            .to_compile_error()
            .into(),
    }
}

fn get_crate_path(attrs: &[syn::Attribute]) -> proc_macro2::TokenStream {
    // Check if there's a #[widget_crate(path)] attribute
    for attr in attrs {
        if attr.path.is_ident("widget_crate")
            && let Ok(syn::Meta::List(meta_list)) = attr.parse_meta()
            && let Some(syn::NestedMeta::Meta(syn::Meta::Path(path))) = meta_list.nested.first()
        {
            return quote!(#path);
        }
    }

    // Default: try to detect if we're in frostsnap_widgets itself
    // by using crate:: which will work within the crate, and users outside
    // can specify #[widget_crate(frostsnap_widgets)]
    quote!(crate)
}

fn derive_widget_for_enum(
    name: &syn::Ident,
    impl_generics: &syn::ImplGenerics,
    ty_generics: &syn::TypeGenerics,
    where_clause: &Option<&syn::WhereClause>,
    data_enum: &syn::DataEnum,
    crate_path: &proc_macro2::TokenStream,
) -> TokenStream {
    // Generate match arms for each method
    let draw_arms = generate_match_arms(&data_enum.variants, quote!(draw(target, current_time)));
    let set_constraints_arms =
        generate_match_arms(&data_enum.variants, quote!(set_constraints(max_size)));
    let sizing_arms = generate_match_arms(&data_enum.variants, quote!(sizing()));
    let handle_touch_arms = generate_match_arms(
        &data_enum.variants,
        quote!(handle_touch(point, current_time, is_release)),
    );
    let handle_vertical_drag_arms = generate_match_arms(
        &data_enum.variants,
        quote!(handle_vertical_drag(prev_y, new_y, is_release)),
    );
    let force_full_redraw_arms =
        generate_match_arms(&data_enum.variants, quote!(force_full_redraw()));

    // Generate match arms for widget_name
    let widget_name_arms = data_enum.variants.iter().map(|variant| {
        let variant_ident = &variant.ident;
        let variant_name = variant_ident.to_string();
        match &variant.fields {
            Fields::Unit => quote! {
                Self::#variant_ident => #variant_name,
            },
            Fields::Unnamed(_) => quote! {
                Self::#variant_ident(..) => #variant_name,
            },
            Fields::Named(_) => quote! {
                Self::#variant_ident { .. } => #variant_name,
            },
        }
    });

    // Generate both DynWidget and Widget trait implementations
    let expanded = quote! {
        impl #impl_generics #name #ty_generics #where_clause {
            /// Returns the name of the current widget variant
            pub fn widget_name(&self) -> &'static str {
                match self {
                    #(#widget_name_arms)*
                }
            }
        }

        impl #impl_generics #crate_path::DynWidget for #name #ty_generics #where_clause {
            fn set_constraints(&mut self, max_size: embedded_graphics::geometry::Size) {
                match self {
                    #(#set_constraints_arms)*
                }
            }

            fn sizing(&self) -> #crate_path::Sizing {
                match self {
                    #(#sizing_arms)*
                }
            }

            fn handle_touch(
                &mut self,
                point: embedded_graphics::geometry::Point,
                current_time: #crate_path::Instant,
                is_release: bool,
            ) -> Option<#crate_path::KeyTouch> {
                match self {
                    #(#handle_touch_arms)*
                }
            }

            fn handle_vertical_drag(&mut self, prev_y: Option<u32>, new_y: u32, is_release: bool) {
                match self {
                    #(#handle_vertical_drag_arms)*
                }
            }

            fn force_full_redraw(&mut self) {
                match self {
                    #(#force_full_redraw_arms)*
                }
            }
        }

        impl #impl_generics #crate_path::Widget for #name #ty_generics #where_clause {
            type Color = embedded_graphics::pixelcolor::Rgb565;

            fn draw<D>(
                &mut self,
                target: &mut #crate_path::SuperDrawTarget<D, Self::Color>,
                current_time: #crate_path::Instant,
            ) -> Result<(), D::Error>
            where
                D: embedded_graphics::draw_target::DrawTarget<Color = Self::Color>,
            {
                match self {
                    #(#draw_arms)*
                }
            }
        }
    };

    TokenStream::from(expanded)
}

fn derive_widget_for_struct(
    name: &syn::Ident,
    impl_generics: &syn::ImplGenerics,
    ty_generics: &syn::TypeGenerics,
    where_clause: &Option<&syn::WhereClause>,
    data_struct: &syn::DataStruct,
    attrs: &[syn::Attribute],
    crate_path: &proc_macro2::TokenStream,
) -> TokenStream {
    // Find the field to delegate to
    let (delegate_field, field_type) =
        match find_delegate_field_with_type(&data_struct.fields, attrs) {
            Some(result) => result,
            None => {
                return syn::Error::new_spanned(
                    name,
                    "Struct must have either:\n\
                 - Exactly one field (for automatic delegation), or\n\
                 - A field marked with #[widget_delegate], or\n\
                 - The struct itself marked with #[widget_delegate(field_name)]",
                )
                .to_compile_error()
                .into();
            }
        };

    // Generate both DynWidget and Widget trait implementations
    let expanded = quote! {
        impl #impl_generics #crate_path::DynWidget for #name #ty_generics #where_clause {
            fn set_constraints(&mut self, max_size: embedded_graphics::geometry::Size) {
                self.#delegate_field.set_constraints(max_size)
            }

            fn sizing(&self) -> #crate_path::Sizing {
                self.#delegate_field.sizing()
            }

            fn handle_touch(
                &mut self,
                point: embedded_graphics::geometry::Point,
                current_time: #crate_path::Instant,
                is_release: bool,
            ) -> Option<#crate_path::KeyTouch> {
                self.#delegate_field.handle_touch(point, current_time, is_release)
            }

            fn handle_vertical_drag(&mut self, prev_y: Option<u32>, new_y: u32, is_release: bool) {
                self.#delegate_field.handle_vertical_drag(prev_y, new_y, is_release)
            }

            fn force_full_redraw(&mut self) {
                self.#delegate_field.force_full_redraw()
            }
        }

        impl #impl_generics #crate_path::Widget for #name #ty_generics #where_clause {
            type Color = <#field_type as #crate_path::Widget>::Color;

            fn draw<D>(
                &mut self,
                target: &mut #crate_path::SuperDrawTarget<D, Self::Color>,
                current_time: #crate_path::Instant,
            ) -> Result<(), D::Error>
            where
                D: embedded_graphics::draw_target::DrawTarget<Color = Self::Color>,
            {
                self.#delegate_field.draw(target, current_time)
            }
        }
    };

    TokenStream::from(expanded)
}

fn find_delegate_field_with_type(
    fields: &syn::Fields,
    struct_attrs: &[syn::Attribute],
) -> Option<(proc_macro2::TokenStream, syn::Type)> {
    // First check if the struct has #[widget_delegate(field_name)]
    for attr in struct_attrs {
        if attr.path.is_ident("widget_delegate")
            && let Ok(syn::Meta::List(meta_list)) = attr.parse_meta()
            && let Some(syn::NestedMeta::Meta(syn::Meta::Path(path))) = meta_list.nested.first()
            && let Some(ident) = path.get_ident()
        {
            // Find the field with this name to get its type
            if let Fields::Named(fields) = fields {
                for field in &fields.named {
                    if field.ident.as_ref() == Some(ident) {
                        return Some((quote!(#ident), field.ty.clone()));
                    }
                }
            }
        }
    }

    match fields {
        Fields::Named(fields) => {
            // Check if any field has #[widget_delegate] attribute
            for field in &fields.named {
                for attr in &field.attrs {
                    if attr.path.is_ident("widget_delegate")
                        && let Some(field_name) = &field.ident
                    {
                        return Some((quote!(#field_name), field.ty.clone()));
                    }
                }
            }

            // If there's only one field, use it
            if fields.named.len() == 1
                && let Some(field) = fields.named.first()
                && let Some(field_name) = &field.ident
            {
                return Some((quote!(#field_name), field.ty.clone()));
            }
        }
        Fields::Unnamed(fields) => {
            // For tuple structs, use the first field if there's only one
            if fields.unnamed.len() == 1
                && let Some(field) = fields.unnamed.first()
            {
                return Some((quote!(0), field.ty.clone()));
            }
        }
        Fields::Unit => {}
    }

    None
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
                Fields::Named(fields) => {
                    // Check if any field has #[widget_delegate] attribute
                    let delegate_field = fields.named.iter().find_map(|field| {
                        for attr in &field.attrs {
                            if attr.path.is_ident("widget_delegate") {
                                return field.ident.as_ref();
                            }
                        }
                        None
                    });
                    if let Some(field_name) = delegate_field {
                        // Use the field marked with #[widget_delegate]
                        quote! {
                            Self::#variant_ident { #field_name, .. } => #field_name.#method_call,
                        }
                    } else {
                        // Fall back to looking for a field named 'widget'
                        quote! {
                            Self::#variant_ident { widget, .. } => widget.#method_call,
                        }
                    }
                }
            }
        })
        .collect()
}

#[proc_macro]
pub fn hex(input: TokenStream) -> TokenStream {
    let input_str = parse_macro_input!(input as syn::LitStr);
    let hex_str = input_str.value();

    let hex_str = hex_str.trim();

    if hex_str.len() % 2 != 0 {
        return syn::Error::new_spanned(
            input_str,
            format!(
                "hex string must have even length, got {} characters",
                hex_str.len()
            ),
        )
        .to_compile_error()
        .into();
    }

    let mut bytes = Vec::new();
    for i in (0..hex_str.len()).step_by(2) {
        let byte_str = &hex_str[i..i + 2];
        match u8::from_str_radix(byte_str, 16) {
            Ok(byte) => bytes.push(byte),
            Err(_) => {
                return syn::Error::new_spanned(
                    input_str,
                    format!("invalid hex digit in '{}'", byte_str),
                )
                .to_compile_error()
                .into();
            }
        }
    }

    let byte_literals = bytes.iter().map(|b| quote!(#b));

    let expanded = quote! {
        [#(#byte_literals),*]
    };

    TokenStream::from(expanded)
}

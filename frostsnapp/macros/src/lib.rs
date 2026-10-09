use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{
    parse::Parse, parse::ParseStream, parse_macro_input, Fields, GenericArgument, ItemStruct,
    PathArguments, Type,
};

/// Exposes a `Broadcast<T>` or `BehaviorBroadcast<T>` to Dart as a handle with
/// `Stream<T> watch()`, which registers a Rust sink on `listen` and unregisters it on `cancel`.
///
/// ```ignore
/// use flutter_rust_bridge::frb;
/// broadcast_handle! { pub struct FooBcast(pub Broadcast<Foo>); }
/// ```
///
/// The call site must import `frb`: flutter_rust_bridge's codegen finds its attributes by that
/// bare name.
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
        if attr.path().is_ident("frb") {
            return Err(syn::Error::new_spanned(
                attr,
                "broadcast_handle! emits its own #[frb(...)] attributes",
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

    // A request struct rather than a bare `StreamSink` argument: frb turns any function taking a
    // `StreamSink` into one that returns a `Stream`, and attach must return the registration id.
    let req_ident = format_ident!("{}WatchReq", struct_ident);
    let dart_type_str = dart_type(&element_ty)?;
    let dart_code = build_handle_dart_code(&req_ident, &dart_type_str);
    let dart_lit = syn::LitStr::new(&dart_code, proc_macro2::Span::call_site());

    item_struct
        .attrs
        .push(syn::parse_quote!(#[frb(non_opaque)]));
    item_struct
        .attrs
        .push(syn::parse_quote!(#[frb(dart_code = #dart_lit)]));

    Ok(quote! {
        pub struct #req_ident {
            pub sink: crate::frb_generated::StreamSink<#element_ty>,
        }

        #item_struct

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
    })
}

fn element_type_from_inner(ty: &Type) -> syn::Result<Type> {
    const EXPECTED: &str =
        "broadcast_handle! field type must be `Broadcast<T>` or `BehaviorBroadcast<T>`";
    let Type::Path(path) = ty else {
        return Err(syn::Error::new_spanned(ty, EXPECTED));
    };
    let Some(last) = path.path.segments.last() else {
        return Err(syn::Error::new_spanned(ty, EXPECTED));
    };
    if last.ident != "Broadcast" && last.ident != "BehaviorBroadcast" {
        return Err(syn::Error::new_spanned(&last.ident, EXPECTED));
    }
    let PathArguments::AngleBracketed(args) = &last.arguments else {
        return Err(syn::Error::new_spanned(&last.arguments, EXPECTED));
    };
    match args.args.iter().collect::<Vec<_>>().as_slice() {
        [GenericArgument::Type(item_ty)] => Ok(item_ty.clone()),
        _ => Err(syn::Error::new_spanned(&args.args, EXPECTED)),
    }
}

fn build_handle_dart_code(req_ident: &syn::Ident, dart_type: &str) -> String {
    format!(
        "\n  Stream<{dart_type}> watch() =>\n      rustBroadcastStream<{dart_type}>(\n        attach: (sink) => frbAttachWatch(req: {req_ident}(sink: sink)),\n        detach: (id) => detach(id: id as SinkRegistrationId),\n      );\n",
    )
}

/// frb's Dart name for `ty`, which the injected `watch()` has to spell out itself.
fn dart_type(ty: &Type) -> syn::Result<String> {
    if let Type::Tuple(tuple) = ty {
        if tuple.elems.is_empty() {
            return Ok("void".to_string());
        }
    }
    let unsupported = || {
        syn::Error::new_spanned(
            ty,
            "broadcast_handle! element type must be `()`, a primitive, `String` or a non-generic named type",
        )
    };
    let Type::Path(path) = ty else {
        return Err(unsupported());
    };
    let Some(last) = path.path.segments.last() else {
        return Err(unsupported());
    };
    if !last.arguments.is_empty() {
        return Err(unsupported());
    }
    Ok(match last.ident.to_string().as_str() {
        "i8" | "i16" | "i32" | "i64" | "isize" | "u8" | "u16" | "u32" | "u64" | "usize" => {
            "int".to_string()
        }
        "f32" | "f64" => "double".to_string(),
        other => other.to_string(),
    })
}

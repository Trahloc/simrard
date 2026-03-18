//! Derive ActionBuilder on a given struct
use proc_macro2::TokenStream;
use quote::quote;
use syn::{parse_macro_input, DeriveInput, Expr, Ident, Lit, LitStr, Meta};

/// Derive ActionBuilder on a struct that implements Component + Clone
pub fn action_builder_impl(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    let label = get_label(&input);

    let component_name = input.ident;
    let generics = input.generics;
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let component_string = component_name.to_string();
    let build_method = build_method(&component_name, &ty_generics);
    let label_method = label_method(
        label.unwrap_or_else(|| LitStr::new(&component_string, component_name.span())),
    );

    let generated = quote! {
        impl #impl_generics ::simrard_lib_utility_ai::actions::ActionBuilder for #component_name #ty_generics #where_clause {
            #build_method
            #label_method
        }
    };

    proc_macro::TokenStream::from(generated)
}

fn get_label(input: &DeriveInput) -> Option<LitStr> {
    let mut label: Option<LitStr> = None;
    let attrs = &input.attrs;
    for option in attrs {
        if let Meta::NameValue(meta_name_value) = &option.meta {
            if meta_name_value.path.is_ident("action_label") {
                if let Expr::Lit(expr_lit) = &meta_name_value.value {
                    if let Lit::Str(lit_str) = &expr_lit.lit {
                        label = Some(lit_str.clone());
                    } else {
                        panic!("Must specify a string for the `action_label` attribute")
                    }
                } else {
                    panic!("Must specify a string for the `action_label` attribute")
                }
            }
        }
    }
    label
}

fn build_method(component_name: &Ident, ty_generics: &syn::TypeGenerics) -> TokenStream {
    let turbofish = ty_generics.as_turbofish();

    quote! {
        fn build(&self, cmd: &mut ::bevy::prelude::Commands, action: ::bevy::prelude::Entity, _actor: ::bevy::prelude::Entity) {
            cmd.entity(action).insert(#component_name #turbofish::clone(self));
        }
    }
}

fn label_method(label: LitStr) -> TokenStream {
    quote! {
        fn label(&self) -> ::std::option::Option<&str> {
            ::std::option::Option::Some(#label)
        }
    }
}

use convert_case::{Case, Casing};
use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, FnArg, ImplItem, ItemImpl, Pat, ReturnType};

/// Derive macro that generates JsonAction implementations for service methods
///
/// Example:
/// ```rust
/// #[json_actions]
/// impl AnvilService {
///     #[json_action]
///     pub async fn mine_blocks(&self, count: u64) -> Result<Vec<String>, Error> {
///         // implementation
///     }
/// }
/// ```
#[proc_macro_attribute]
pub fn json_actions(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let impl_block = parse_macro_input!(item as ItemImpl);

    let mut action_structs = vec![];
    let mut action_impls = vec![];
    let mut registration_calls = vec![];
    let mut dispatch_arms = vec![];

    let self_type = &impl_block.self_ty;

    // Process each method in the impl block
    for item in &impl_block.items {
        if let ImplItem::Fn(method) = item {
            // Check if method has #[json_action] attribute
            let has_json_action = method
                .attrs
                .iter()
                .any(|attr| attr.path().is_ident("json_action"));

            if !has_json_action {
                continue;
            }

            let method_name = &method.sig.ident;
            let method_name_str = method_name.to_string();

            // Generate PascalCase struct name from snake_case method name
            let struct_name_str = format!("{}Action", method_name_str.to_case(Case::Pascal));
            let struct_name = syn::Ident::new(&struct_name_str, method_name.span());

            // Extract parameters (skip &self)
            let mut param_fields = vec![];
            let mut param_names = vec![];
            let mut param_types = vec![];

            for arg in method.sig.inputs.iter().skip(1) {
                if let FnArg::Typed(pat_type) = arg {
                    if let Pat::Ident(pat_ident) = &*pat_type.pat {
                        let param_name = &pat_ident.ident;
                        let param_type = &pat_type.ty;

                        param_fields.push(quote! {
                            pub #param_name: #param_type
                        });
                        param_names.push(param_name);
                        param_types.push(param_type);
                    }
                }
            }

            // Extract return type
            let return_type = match &method.sig.output {
                ReturnType::Type(_, ty) => {
                    // Extract T from Result<T, Error>
                    if let syn::Type::Path(type_path) = &**ty {
                        if let Some(segment) = type_path.path.segments.last() {
                            if segment.ident == "Result" {
                                if let syn::PathArguments::AngleBracketed(args) = &segment.arguments
                                {
                                    if let Some(syn::GenericArgument::Type(inner_ty)) =
                                        args.args.first()
                                    {
                                        quote! { #inner_ty }
                                    } else {
                                        quote! { () }
                                    }
                                } else {
                                    quote! { () }
                                }
                            } else {
                                quote! { #ty }
                            }
                        } else {
                            quote! { () }
                        }
                    } else {
                        quote! { () }
                    }
                }
                ReturnType::Default => quote! { () },
            };

            // Generate action struct
            action_structs.push(quote! {
                #[derive(Debug, Clone, serde::Deserialize, serde::Serialize, schemars::JsonSchema)]
                #[allow(missing_docs)]
                pub struct #struct_name {
                    #(#param_fields),*
                }
            });

            // Generate JsonAction implementation
            let method_call = if param_names.is_empty() {
                quote! { service.#method_name().await }
            } else {
                quote! { service.#method_name(#(self.#param_names),*).await }
            };

            action_impls.push(quote! {
                #[async_trait::async_trait]
                impl harness_core::action::JsonAction<#self_type> for #struct_name {
                    type Response = #return_type;
                    fn action_name() -> &'static str {
                        #method_name_str
                    }
                    async fn execute(self, service: &#self_type) -> std::result::Result<Self::Response, harness_core::Error> {
                        #method_call
                    }
                }
            });

            // Generate registration call
            registration_calls.push(quote! {
                registry.register::<#struct_name, #self_type>()?;
            });

            // Generate dispatch arm
            dispatch_arms.push(quote! {
                #method_name_str => {
                    let action: #struct_name = serde_json::from_value(input)
                        .map_err(|e| harness_core::Error::service_type(format!("Failed to deserialize action: {}", e)))?;
                    let result = action.execute(self).await?;
                    serde_json::to_value(result)
                        .map_err(|e| harness_core::Error::service_type(format!("Failed to serialize response: {}", e)))
                }
            });
        }
    }

    // Generate the register_actions and dispatch_json_action methods
    let service_impls = quote! {
        impl harness_core::action::ServiceJsonActions for #self_type {
            fn register_actions(registry: &mut harness_core::action::JsonActionRegistry) -> std::result::Result<(), harness_core::Error> {
                #(#registration_calls)*
                Ok(())
            }
        }

        impl #self_type {
            /// Dispatch a JSON action to this service (generated by json_actions macro)
            pub async fn dispatch_json_action(
                &self,
                action_name: &str,
                input: serde_json::Value,
            ) -> std::result::Result<serde_json::Value, harness_core::Error> {
                match action_name {
                    #(#dispatch_arms)*
                    _ => Err(harness_core::Error::service_type(format!("Unknown action: {}", action_name))),
                }
            }
        }

        #[async_trait::async_trait]
        impl harness_core::service::HasDispatchJson for #self_type {
            async fn dispatch_json_action(
                &self,
                action_name: &str,
                input: serde_json::Value,
            ) -> std::result::Result<serde_json::Value, harness_core::Error> {
                self.dispatch_json_action(action_name, input).await
            }
        }
    };

    // Combine everything
    let output = quote! {
        #impl_block

        #(#action_structs)*

        #(#action_impls)*

        #service_impls
    };

    TokenStream::from(output)
}

/// Attribute to mark a method for JSON action generation
/// This is used with #[json_actions] on the impl block
#[proc_macro_attribute]
pub fn json_action(_attr: TokenStream, item: TokenStream) -> TokenStream {
    // This is just a marker attribute, it doesn't transform the method
    item
}

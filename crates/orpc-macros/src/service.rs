use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{
    FnArg, Ident, ItemTrait, ReturnType, Token, TraitItem, Type, parse::Parse, parse::ParseStream,
};

/// Parsed attribute: `#[orpc_service(context = AppCtx)]`
pub(crate) struct ServiceAttr {
    pub context_type: Type,
}

impl Parse for ServiceAttr {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let ident: Ident = input.parse()?;
        if ident != "context" {
            return Err(syn::Error::new(ident.span(), "expected `context`"));
        }
        let _: Token![=] = input.parse()?;
        let context_type: Type = input.parse()?;
        Ok(ServiceAttr { context_type })
    }
}

/// Extracted procedure info from a trait method.
struct ProcedureInfo {
    method_name: Ident,
    input_type: Option<Type>,
    output_type: Type,
}

/// Parse a trait and generate server router function + client struct.
pub(crate) fn expand_service(attr: ServiceAttr, item: ItemTrait) -> syn::Result<TokenStream> {
    let trait_name = &item.ident;
    let trait_vis = &item.vis;
    let ctx_type = &attr.context_type;

    let procedures = extract_procedures(&item, ctx_type)?;

    let transformed_trait = desugar_async_trait(&item);
    let router_fn = generate_router_fn(trait_name, trait_vis, ctx_type, &procedures);
    let client_struct = generate_client_struct(trait_name, trait_vis, &procedures);

    Ok(quote! {
        #transformed_trait

        #router_fn

        #client_struct
    })
}

/// Transform `async fn` methods in the trait to `fn() -> impl Future + Send`.
///
/// This is necessary because `async fn` in traits produces futures that
/// are not `Send` by default, but orpc handlers require `Send`.
fn desugar_async_trait(item: &ItemTrait) -> TokenStream {
    let attrs = &item.attrs;
    let vis = &item.vis;
    let ident = &item.ident;
    let supertraits = &item.supertraits;
    let generics = &item.generics;

    let items: Vec<TokenStream> = item
        .items
        .iter()
        .map(|trait_item| {
            let TraitItem::Fn(method) = trait_item else {
                return quote! { #trait_item };
            };

            if method.sig.asyncness.is_none() {
                return quote! { #trait_item };
            }

            // Desugar: async fn foo() -> T  →  fn foo() -> impl Future<Output = T> + Send
            let method_attrs = &method.attrs;
            let method_sig = &method.sig;
            let fn_name = &method_sig.ident;
            let inputs = &method_sig.inputs;
            let generics = &method_sig.generics;

            let output_type = match &method_sig.output {
                ReturnType::Default => quote! { () },
                ReturnType::Type(_, ty) => quote! { #ty },
            };

            quote! {
                #(#method_attrs)*
                fn #fn_name #generics(#inputs) -> impl std::future::Future<Output = #output_type> + Send;
            }
        })
        .collect();

    let colon = if supertraits.is_empty() {
        quote! {}
    } else {
        quote! { : }
    };

    quote! {
        #(#attrs)*
        #vis trait #ident #generics #colon #supertraits {
            #(#items)*
        }
    }
}

fn extract_procedures(item: &ItemTrait, ctx_type: &Type) -> syn::Result<Vec<ProcedureInfo>> {
    let mut procedures = Vec::new();

    for trait_item in &item.items {
        let TraitItem::Fn(method) = trait_item else {
            continue;
        };

        let method_name = method.sig.ident.clone();
        let inputs: Vec<_> = method.sig.inputs.iter().collect();

        // Validate: first param must be &self
        if inputs.is_empty() {
            return Err(syn::Error::new_spanned(
                &method.sig,
                "method must have &self as first parameter",
            ));
        }
        match &inputs[0] {
            FnArg::Receiver(_) => {}
            other => {
                return Err(syn::Error::new_spanned(
                    other,
                    "first parameter must be &self",
                ));
            }
        }

        // Second param: ctx (must match context type, we just skip it)
        if inputs.len() < 2 {
            return Err(syn::Error::new_spanned(
                &method.sig,
                format!(
                    "method must have ctx parameter of type {}",
                    quote!(#ctx_type)
                ),
            ));
        }

        // Third param (optional): input type
        let input_type = if inputs.len() >= 3 {
            match &inputs[2] {
                FnArg::Typed(pat_type) => Some((*pat_type.ty).clone()),
                _ => None,
            }
        } else {
            None
        };

        // Return type: extract T from Result<T, _>
        let output_type = extract_result_ok_type(&method.sig.output)?;

        procedures.push(ProcedureInfo {
            method_name,
            input_type,
            output_type,
        });
    }

    Ok(procedures)
}

/// Extract T from `-> Result<T, E>`.
fn extract_result_ok_type(ret: &ReturnType) -> syn::Result<Type> {
    let ReturnType::Type(_, ty) = ret else {
        return Err(syn::Error::new_spanned(
            ret,
            "method must return Result<T, ORPCError>",
        ));
    };

    // Match Type::Path where last segment is Result<T, E>
    if let Type::Path(type_path) = ty.as_ref()
        && let Some(segment) = type_path.path.segments.last()
        && segment.ident == "Result"
        && let syn::PathArguments::AngleBracketed(args) = &segment.arguments
        && let Some(syn::GenericArgument::Type(ok_type)) = args.args.first()
    {
        return Ok(ok_type.clone());
    }

    Err(syn::Error::new_spanned(
        ty,
        "return type must be Result<T, ORPCError>",
    ))
}

fn generate_router_fn(
    trait_name: &Ident,
    vis: &syn::Visibility,
    ctx_type: &Type,
    procedures: &[ProcedureInfo],
) -> TokenStream {
    let fn_name = format_ident!("{}_router", to_snake_case(&trait_name.to_string()));

    let procedure_defs: Vec<TokenStream> = procedures
        .iter()
        .enumerate()
        .map(|(i, proc)| {
            let api_clone = format_ident!("__api_{}", i);
            let var_name = &proc.method_name;
            let method_name = &proc.method_name;

            if let Some(input_type) = &proc.input_type {
                quote! {
                    let #api_clone = __api.clone();
                    let #var_name = orpc::os::<#ctx_type>()
                        .input(orpc::Identity::<#input_type>::new())
                        .handler(move |ctx: #ctx_type, input: #input_type| {
                            let api = #api_clone.clone();
                            async move { api.#method_name(ctx, input).await }
                        });
                }
            } else {
                quote! {
                    let #api_clone = __api.clone();
                    let #var_name = orpc::os::<#ctx_type>()
                        .handler(move |ctx: #ctx_type, _input: ()| {
                            let api = #api_clone.clone();
                            async move { api.#method_name(ctx).await }
                        });
                }
            }
        })
        .collect();

    let router_inserts: Vec<TokenStream> = procedures
        .iter()
        .map(|proc| {
            let name = &proc.method_name;
            let path = name.to_string();
            quote! { #path => #name }
        })
        .collect();

    quote! {
        /// Build an oRPC [`Router`](orpc::Router) from a [`#trait_name`] implementation.
        #vis fn #fn_name<T: #trait_name + Send + Sync + 'static>(
            api: T,
        ) -> orpc::Router<#ctx_type> {
            let __api = std::sync::Arc::new(api);
            #(#procedure_defs)*
            orpc::router! {
                #(#router_inserts),*
            }
        }
    }
}

fn generate_client_struct(
    trait_name: &Ident,
    vis: &syn::Visibility,
    procedures: &[ProcedureInfo],
) -> TokenStream {
    let client_name = format_ident!("{}Client", trait_name);

    let methods: Vec<TokenStream> = procedures
        .iter()
        .map(|proc| {
            let method_name = &proc.method_name;
            let output_type = &proc.output_type;
            let path = method_name.to_string();

            if let Some(input_type) = &proc.input_type {
                quote! {
                    pub async fn #method_name(
                        &self,
                        input: &#input_type,
                    ) -> Result<#output_type, orpc_client::ClientError> {
                        self.client.call(#path, input).await
                    }
                }
            } else {
                quote! {
                    pub async fn #method_name(&self) -> Result<#output_type, orpc_client::ClientError> {
                        self.client.call(#path, &()).await
                    }
                }
            }
        })
        .collect();

    quote! {
        /// Typed RPC client generated from [`#trait_name`].
        #vis struct #client_name<L: orpc_client::Link = orpc_client::RpcLink> {
            client: orpc_client::Client<L>,
        }

        impl #client_name<orpc_client::RpcLink> {
            /// Create a new client targeting the given base URL.
            pub fn new(base_url: impl Into<String>) -> Self {
                Self {
                    client: orpc_client::Client::new(base_url),
                }
            }
        }

        impl<L: orpc_client::Link> #client_name<L> {
            /// Create a client with a custom [`Link`](orpc_client::Link) implementation.
            pub fn with_link(link: L) -> Self {
                Self {
                    client: orpc_client::Client::with_link(link),
                }
            }

            #(#methods)*
        }
    }
}

fn to_snake_case(s: &str) -> String {
    let mut result = String::new();
    for (i, ch) in s.chars().enumerate() {
        if ch.is_uppercase() {
            if i > 0 {
                result.push('_');
            }
            result.push(ch.to_lowercase().next().unwrap());
        } else {
            result.push(ch);
        }
    }
    result
}

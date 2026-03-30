mod service;

use proc_macro::TokenStream;
use syn::parse_macro_input;

/// Define an oRPC service from a trait.
///
/// Generates both a server router function and a typed client struct.
///
/// # Attributes
///
/// - `context = Type` (required): The server context type passed to handlers.
///
/// # Example
///
/// ```ignore
/// use orpc::orpc_service;
///
/// #[orpc_service(context = AppCtx)]
/// pub trait PlanetApi {
///     async fn ping(&self, ctx: AppCtx) -> Result<String, ORPCError>;
///     async fn find_planet(&self, ctx: AppCtx, input: FindPlanetInput) -> Result<Planet, ORPCError>;
/// }
///
/// // Server: implement the trait and build a Router
/// struct MyApi;
/// impl PlanetApi for MyApi {
///     async fn ping(&self, _ctx: AppCtx) -> Result<String, ORPCError> { Ok("pong".into()) }
///     async fn find_planet(&self, ctx: AppCtx, input: FindPlanetInput) -> Result<Planet, ORPCError> { /* ... */ }
/// }
/// let router = planet_api_router(MyApi);
///
/// // Client: typed RPC client
/// let client = PlanetApiClient::new("http://localhost:3000/rpc");
/// let planet = client.find_planet(&FindPlanetInput { name: "Earth".into() }).await?;
/// ```
///
/// # Method Signatures
///
/// Each method must follow one of these patterns:
///
/// - **No input**: `async fn name(&self, ctx: Ctx) -> Result<Output, ORPCError>`
/// - **With input**: `async fn name(&self, ctx: Ctx, input: Input) -> Result<Output, ORPCError>`
///
/// The method name becomes the RPC procedure path (e.g., `find_planet` → `"find_planet"`).
#[proc_macro_attribute]
pub fn orpc_service(attr: TokenStream, item: TokenStream) -> TokenStream {
    let attr = parse_macro_input!(attr as service::ServiceAttr);
    let item = parse_macro_input!(item as syn::ItemTrait);

    match service::expand_service(attr, item) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

use futures_util::StreamExt;
use orpc::*;
use serde::{Deserialize, Serialize};

// --- Context types ---

struct AppCtx {
    user_id: u32,
}

struct AuthCtx {
    user: String,
    db: String,
}

// --- Input/Output types ---

#[derive(Debug, Deserialize, Serialize)]
struct GreetInput {
    name: String,
}

#[derive(Debug, Deserialize, Serialize, PartialEq)]
struct Planet {
    name: String,
    radius: u32,
}

// --- Handlers ---

async fn greet(ctx: AuthCtx, input: GreetInput) -> Result<String, ORPCError> {
    Ok(format!("Hello {}, from {} via {}!", input.name, ctx.user, ctx.db))
}

async fn find_planet(_ctx: AppCtx, input: String) -> Result<Planet, ORPCError> {
    match input.as_str() {
        "Earth" => Ok(Planet {
            name: "Earth".into(),
            radius: 6371,
        }),
        _ => Err(ORPCError::not_found(format!("Planet '{input}' not found"))),
    }
}

async fn ping(_ctx: AppCtx, _input: ()) -> Result<String, ORPCError> {
    Ok("pong".into())
}

// --- Tests ---

/// Full middleware chain: AppCtx → AuthCtx → handler
#[tokio::test]
async fn end_to_end_with_middleware() {
    let auth_mw = |ctx: AppCtx, mw: MiddlewareCtx<AuthCtx>| {
        Box::pin(async move {
            mw.next(AuthCtx {
                user: format!("user-{}", ctx.user_id),
                db: "postgres".into(),
            })
            .await
        }) as BoxFuture<'static, Result<MiddlewareOutput, ProcedureError>>
    };

    let proc = os::<AppCtx>()
        .use_middleware(auth_mw)
        .route(Route::post("/greet"))
        .input(Identity::<GreetInput>::new())
        .handler(greet);

    let erased: ErasedProcedure<AppCtx> = proc.into_erased();

    let input = DynInput::from_value(serde_json::json!({"name": "World"}));
    let mut stream = erased.exec(AppCtx { user_id: 7 }, input);
    let result = stream.next().await.unwrap().unwrap();
    assert_eq!(
        result.to_value().unwrap(),
        serde_json::json!("Hello World, from user-7 via postgres!")
    );
}

/// No middleware, direct handler
#[tokio::test]
async fn no_middleware_procedure() {
    let proc = os::<AppCtx>()
        .input(Identity::<String>::new())
        .handler(find_planet);

    let erased = proc.into_erased();
    let input = DynInput::from_value(serde_json::json!("Earth"));
    let mut stream = erased.exec(AppCtx { user_id: 0 }, input);
    let result = stream.next().await.unwrap().unwrap();
    let planet: Planet = serde_json::from_value(result.to_value().unwrap()).unwrap();
    assert_eq!(
        planet,
        Planet {
            name: "Earth".into(),
            radius: 6371
        }
    );
}

/// Handler returns error → ProcedureError::Resolver
#[tokio::test]
async fn handler_error_propagation() {
    let proc = os::<AppCtx>()
        .input(Identity::<String>::new())
        .handler(find_planet);

    let erased = proc.into_erased();
    let input = DynInput::from_value(serde_json::json!("Vulcan"));
    let mut stream = erased.exec(AppCtx { user_id: 0 }, input);
    let result = stream.next().await.unwrap();
    assert!(matches!(result, Err(ProcedureError::Resolver(_))));
}

/// Router with multiple procedures
#[tokio::test]
async fn router_integration() {
    let ping_proc = os::<AppCtx>().handler(ping);
    let find_proc = os::<AppCtx>()
        .input(Identity::<String>::new())
        .handler(find_planet);

    let r: Router<AppCtx> = router! {
        "ping" : ping_proc,
        "planet.find" : find_proc,
    };

    assert_eq!(r.len(), 2);

    // Execute ping
    let ping = r.get("ping").unwrap();
    let input = DynInput::from_value(serde_json::json!(null));
    let mut stream = ping.exec(AppCtx { user_id: 0 }, input);
    let result = stream.next().await.unwrap().unwrap();
    assert_eq!(result.to_value().unwrap(), serde_json::json!("pong"));

    // Execute planet.find
    let find = r.get("planet.find").unwrap();
    let input = DynInput::from_value(serde_json::json!("Earth"));
    let mut stream = find.exec(AppCtx { user_id: 0 }, input);
    let result = stream.next().await.unwrap().unwrap();
    let planet: Planet = serde_json::from_value(result.to_value().unwrap()).unwrap();
    assert_eq!(planet.name, "Earth");
}

/// Router with nested sub-router
#[tokio::test]
async fn router_nested() {
    let inner: Router<AppCtx> = router! {
        "find" : os::<AppCtx>().input(Identity::<String>::new()).handler(find_planet),
    };

    let r = router! {
        "ping" : os::<AppCtx>().handler(ping),
    }
    .nest("planet", inner);

    assert_eq!(r.len(), 2);
    assert!(r.get("ping").is_some());
    assert!(r.get("planet.find").is_some());
}

/// Middleware short-circuit via output()
#[tokio::test]
async fn middleware_short_circuit() {
    let cache_mw = |_ctx: (), mw: MiddlewareCtx<()>| {
        Box::pin(async move { mw.output("cached") }) as BoxFuture<'static, Result<MiddlewareOutput, ProcedureError>>
    };

    let proc = os::<()>().use_middleware(cache_mw).handler(
        |_ctx: (), _input: ()| async move { Ok::<_, ORPCError>("should not reach".to_string()) },
    );

    let erased = proc.into_erased();
    let input = DynInput::from_value(serde_json::json!(null));
    let mut stream = erased.exec((), input);
    let result = stream.next().await.unwrap().unwrap();
    assert_eq!(result.to_value().unwrap(), serde_json::json!("cached"));
}

/// Double middleware: () → u32 → String
#[tokio::test]
async fn double_middleware_chain() {
    let mw1 = |_ctx: (), mw: MiddlewareCtx<u32>| {
        Box::pin(async move { mw.next(42u32).await }) as BoxFuture<'static, Result<MiddlewareOutput, ProcedureError>>
    };

    let mw2 = |ctx: u32, mw: MiddlewareCtx<String>| {
        Box::pin(async move { mw.next(format!("val-{ctx}")).await })
            as BoxFuture<'static, Result<MiddlewareOutput, ProcedureError>>
    };

    let proc = os::<()>()
        .use_middleware(mw1)
        .use_middleware(mw2)
        .handler(|ctx: String, _input: ()| async move { Ok::<_, ORPCError>(ctx) });

    let erased = proc.into_erased();
    let input = DynInput::from_value(serde_json::json!(null));
    let mut stream = erased.exec((), input);
    let result = stream.next().await.unwrap().unwrap();
    assert_eq!(result.to_value().unwrap(), serde_json::json!("val-42"));
}

/// ORPCError wire format check
#[test]
fn orpc_error_wire_format() {
    let err = ORPCError::not_found("User not found")
        .with_data(serde_json::json!({"userId": "123"}));
    let json = serde_json::to_value(&err).unwrap();
    assert_eq!(json["code"], "NOT_FOUND");
    assert_eq!(json["status"], 404);
    assert_eq!(json["message"], "User not found");
    assert_eq!(json["data"]["userId"], "123");
}

/// Compile-time type safety: all public types are Send
#[test]
fn all_types_are_send() {
    fn assert_send<T: Send>() {}
    assert_send::<ORPCError>();
    assert_send::<MiddlewareCtx<()>>();
    assert_send::<MiddlewareOutput>();
    assert_send::<Procedure<(), (), (), ORPCError>>();
    assert_send::<Router<()>>();
}

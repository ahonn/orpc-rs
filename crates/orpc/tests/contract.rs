use futures_util::StreamExt;
use orpc::*;
use serde::{Deserialize, Serialize};

// --- Types ---

struct AppCtx {
    db: String,
}

struct AuthCtx {
    db: String,
    user: String,
}

#[derive(Debug, Deserialize, Serialize, PartialEq)]
struct GetUserInput {
    id: u32,
}

#[derive(Debug, Deserialize, Serialize, PartialEq)]
struct CreateUserInput {
    name: String,
}

#[derive(Debug, Deserialize, Serialize, PartialEq)]
struct User {
    id: u32,
    name: String,
}

// --- Handlers ---

async fn get_user_handler(ctx: AppCtx, input: GetUserInput) -> Result<User, ORPCError> {
    Ok(User {
        id: input.id,
        name: format!("User {} from {}", input.id, ctx.db),
    })
}

async fn create_user_handler(ctx: AuthCtx, input: CreateUserInput) -> Result<User, ORPCError> {
    Ok(User {
        id: 1,
        name: format!("{} by {} via {}", input.name, ctx.user, ctx.db),
    })
}

// --- Contract-first workflow ---

#[tokio::test]
async fn contract_first_without_middleware() {
    let contract = oc()
        .route(Route::get("/users/{id}"))
        .input(Identity::<GetUserInput>::new())
        .output(Identity::<User>::new())
        .build();

    let proc = implement::<AppCtx, _, _, _>(contract).handler(get_user_handler);
    let erased = proc.into_erased();

    assert_eq!(erased.route.method, Some(HttpMethod::Get));

    let input = DynInput::from_value(serde_json::json!({"id": 7}));
    let mut stream = erased.exec(AppCtx { db: "pg".into() }, input);
    let result = stream.next().await.unwrap().unwrap();
    let user: User = serde_json::from_value(result.to_value().unwrap()).unwrap();
    assert_eq!(
        user,
        User {
            id: 7,
            name: "User 7 from pg".into()
        }
    );
}

#[tokio::test]
async fn contract_first_with_middleware() {
    let auth_mw = middleware_fn(|ctx: AppCtx, mw: MiddlewareCtx<AuthCtx>| async move {
        mw.next(AuthCtx {
            db: ctx.db,
            user: "admin".into(),
        })
        .await
    });

    let contract = oc()
        .route(Route::post("/users"))
        .input(Identity::<CreateUserInput>::new())
        .output(Identity::<User>::new())
        .build();

    let proc = implement::<AppCtx, _, _, _>(contract)
        .use_middleware(auth_mw)
        .handler(create_user_handler);

    let erased = proc.into_erased();
    let input = DynInput::from_value(serde_json::json!({"name": "Alice"}));
    let mut stream = erased.exec(AppCtx { db: "pg".into() }, input);
    let result = stream.next().await.unwrap().unwrap();
    let user: User = serde_json::from_value(result.to_value().unwrap()).unwrap();
    assert_eq!(user.name, "Alice by admin via pg");
}

#[tokio::test]
async fn contract_procedures_in_router() {
    let get_contract = oc()
        .route(Route::get("/users/{id}"))
        .input(Identity::<GetUserInput>::new())
        .output(Identity::<User>::new())
        .build();

    let list_contract = oc()
        .route(Route::get("/users"))
        .output(Identity::<Vec<User>>::new())
        .build();

    // No .into_erased() needed: router! auto-converts via From
    let r: Router<AppCtx> = router! {
        "getUser" => implement::<AppCtx, _, _, _>(get_contract).handler(get_user_handler),
        "listUsers" => implement::<AppCtx, _, _, _>(list_contract).handler(
            |_ctx: AppCtx, _input: ()| async move { Ok::<_, ORPCError>(vec![]) },
        ),
    };

    assert_eq!(r.len(), 2);

    let proc = r.get("getUser").unwrap();
    let input = DynInput::from_value(serde_json::json!({"id": 1}));
    let mut stream = proc.exec(AppCtx { db: "pg".into() }, input);
    let result = stream.next().await.unwrap().unwrap();
    let user: User = serde_json::from_value(result.to_value().unwrap()).unwrap();
    assert_eq!(user.id, 1);
}

#[tokio::test]
async fn contract_with_double_middleware() {
    struct RawCtx;
    struct LogCtx {
        request_id: String,
    }

    let logging_mw = middleware_fn(|_ctx: RawCtx, mw: MiddlewareCtx<LogCtx>| async move {
        mw.next(LogCtx {
            request_id: "req-123".into(),
        })
        .await
    });

    let auth_mw = middleware_fn(|ctx: LogCtx, mw: MiddlewareCtx<AuthCtx>| async move {
        mw.next(AuthCtx {
            db: "pg".into(),
            user: format!("user({})", ctx.request_id),
        })
        .await
    });

    let contract = oc()
        .input(Identity::<CreateUserInput>::new())
        .output(Identity::<User>::new())
        .build();

    let proc = implement::<RawCtx, _, _, _>(contract)
        .use_middleware(logging_mw)
        .use_middleware(auth_mw)
        .handler(create_user_handler);

    let erased = proc.into_erased();
    let input = DynInput::from_value(serde_json::json!({"name": "Bob"}));
    let mut stream = erased.exec(RawCtx, input);
    let result = stream.next().await.unwrap().unwrap();
    let user: User = serde_json::from_value(result.to_value().unwrap()).unwrap();
    assert_eq!(user.name, "Bob by user(req-123) via pg");
}

#[test]
fn erased_contract_preserves_type_ids() {
    let contract = oc()
        .input(Identity::<GetUserInput>::new())
        .output(Identity::<User>::new())
        .build();

    let erased: ErasedContract = contract.into();
    assert_eq!(erased.input_type_id, std::any::TypeId::of::<GetUserInput>());
    assert_eq!(erased.output_type_id, std::any::TypeId::of::<User>());
}

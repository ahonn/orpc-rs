use std::sync::{Arc, Mutex};

use orpc::*;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use tower_http::cors::CorsLayer;

// --- Context ---

#[derive(Clone)]
struct AppCtx {
    db: Arc<Mutex<PlanetDb>>,
    /// Broadcast channel for real-time planet creation events.
    planet_tx: broadcast::Sender<Planet>,
}

struct PlanetDb {
    planets: Vec<Planet>,
    next_id: u32,
}

impl PlanetDb {
    fn new() -> Self {
        let planets = vec![
            Planet {
                id: 1,
                name: "Mercury".into(),
                radius_km: 2440,
                has_rings: false,
            },
            Planet {
                id: 2,
                name: "Venus".into(),
                radius_km: 6052,
                has_rings: false,
            },
            Planet {
                id: 3,
                name: "Earth".into(),
                radius_km: 6371,
                has_rings: false,
            },
            Planet {
                id: 4,
                name: "Mars".into(),
                radius_km: 3390,
                has_rings: false,
            },
            Planet {
                id: 5,
                name: "Jupiter".into(),
                radius_km: 69911,
                has_rings: true,
            },
            Planet {
                id: 6,
                name: "Saturn".into(),
                radius_km: 58232,
                has_rings: true,
            },
            Planet {
                id: 7,
                name: "Uranus".into(),
                radius_km: 25362,
                has_rings: true,
            },
            Planet {
                id: 8,
                name: "Neptune".into(),
                radius_km: 24622,
                has_rings: true,
            },
        ];
        PlanetDb {
            next_id: 9,
            planets,
        }
    }
}

// --- Types ---

#[derive(Debug, Clone, Deserialize, Serialize)]
struct Planet {
    id: u32,
    name: String,
    radius_km: u32,
    has_rings: bool,
}

#[derive(Debug, Deserialize, Serialize)]
struct FindPlanetInput {
    name: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct CreatePlanetInput {
    name: String,
    radius_km: u32,
    has_rings: bool,
}

// --- Handlers ---

async fn ping(_ctx: AppCtx, _input: ()) -> Result<String, ORPCError> {
    Ok("pong".into())
}

async fn list_planets(ctx: AppCtx, _input: ()) -> Result<Vec<Planet>, ORPCError> {
    let db = ctx.db.lock().unwrap();
    Ok(db.planets.clone())
}

async fn find_planet(ctx: AppCtx, input: FindPlanetInput) -> Result<Planet, ORPCError> {
    let db = ctx.db.lock().unwrap();
    db.planets
        .iter()
        .find(|p| p.name.eq_ignore_ascii_case(&input.name))
        .cloned()
        .ok_or_else(|| ORPCError::not_found(format!("Planet '{}' not found", input.name)))
}

async fn create_planet(ctx: AppCtx, input: CreatePlanetInput) -> Result<Planet, ORPCError> {
    let mut db = ctx.db.lock().unwrap();
    let planet = Planet {
        id: db.next_id,
        name: input.name,
        radius_km: input.radius_km,
        has_rings: input.has_rings,
    };
    db.next_id += 1;
    db.planets.push(planet.clone());
    // Notify all SSE subscribers
    let _ = ctx.planet_tx.send(planet.clone());
    Ok(planet)
}

// --- RPC Router (used by @orpc/client RPCLink) ---

fn build_rpc_router() -> Router<AppCtx> {
    // SSE subscription: real-time stream of newly created planets.
    // Subscribes to the broadcast channel and streams each new planet as an SSE event.
    let planet_stream_proc = ErasedProcedure::new(
        |ctx: AppCtx, _input: DynInput| {
            let mut rx = ctx.planet_tx.subscribe();
            let stream = async_stream::stream! {
                loop {
                    match rx.recv().await {
                        Ok(planet) => yield Ok(DynOutput::new(planet)),
                        Err(broadcast::error::RecvError::Closed) => break,
                        Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    }
                }
            };
            ProcedureStream::from_stream(stream)
        },
        Route::default(),
        Meta::default(),
    );

    let mut r = router! {
        "ping" => os::<AppCtx>().handler(ping),
        "planet" => {
            "list" => os::<AppCtx>().handler(list_planets),
            "find" => os::<AppCtx>()
                .input(Identity::<FindPlanetInput>::new())
                .handler(find_planet),
            "create" => os::<AppCtx>()
                .input(Identity::<CreatePlanetInput>::new())
                .handler(create_planet),
        },
    };
    r._insert("planet.stream", planet_stream_proc);
    r
}

// --- OpenAPI Router (REST-style endpoints) ---

fn build_openapi_router() -> Router<AppCtx> {
    let get_planet = ErasedProcedure::new(
        |ctx: AppCtx, input: DynInput| {
            ProcedureStream::from_future(async move {
                let inp: FindPlanetInput = input.deserialize()?;
                let db = ctx.db.lock().unwrap();
                let planet = db
                    .planets
                    .iter()
                    .find(|p| p.name.eq_ignore_ascii_case(&inp.name))
                    .cloned()
                    .ok_or_else(|| {
                        ORPCError::not_found(format!("Planet '{}' not found", inp.name))
                    });
                Ok(DynOutput::new(planet?))
            })
        },
        Route::get("/planets/{name}"),
        Meta::default(),
    );

    let list_all = ErasedProcedure::new(
        |ctx: AppCtx, _input: DynInput| {
            ProcedureStream::from_future(async move {
                let db = ctx.db.lock().unwrap();
                Ok(DynOutput::new(db.planets.clone()))
            })
        },
        Route::get("/planets"),
        Meta::default(),
    );

    let create = ErasedProcedure::new(
        |ctx: AppCtx, input: DynInput| {
            ProcedureStream::from_future(async move {
                let inp: CreatePlanetInput = input.deserialize()?;
                let mut db = ctx.db.lock().unwrap();
                let planet = Planet {
                    id: db.next_id,
                    name: inp.name,
                    radius_km: inp.radius_km,
                    has_rings: inp.has_rings,
                };
                db.next_id += 1;
                db.planets.push(planet.clone());
                let _ = ctx.planet_tx.send(planet.clone());
                Ok(DynOutput::new(planet))
            })
        },
        Route::post("/planets"),
        Meta::default(),
    );

    Router::new()
        .procedure("getPlanet", get_planet)
        .procedure("listPlanets", list_all)
        .procedure("createPlanet", create)
}

// --- Server ---

#[tokio::main]
async fn main() {
    let db = Arc::new(Mutex::new(PlanetDb::new()));
    let (planet_tx, _) = broadcast::channel::<Planet>(16);

    let rpc_router = build_rpc_router();
    let openapi_router = build_openapi_router();

    println!("RPC procedures:");
    for key in rpc_router.procedures().keys() {
        println!("  POST /rpc/{}", key.replace('.', "/"));
    }
    println!("\nOpenAPI endpoints:");
    for proc in openapi_router.procedures().values() {
        if let (Some(method), Some(path)) = (proc.route.method, &proc.route.path) {
            println!("  {method} /rest{path}");
        }
    }

    let db_clone = db.clone();
    let tx_clone = planet_tx.clone();

    let rpc = orpc_axum::into_router(rpc_router, move |_parts: &http::request::Parts| AppCtx {
        db: db.clone(),
        planet_tx: planet_tx.clone(),
    });

    let openapi = orpc_axum::into_openapi_router(
        openapi_router,
        move |_parts: &http::request::Parts| AppCtx {
            db: db_clone.clone(),
            planet_tx: tx_clone.clone(),
        },
        orpc_axum::OpenAPIConfig::default(),
    );

    let app = axum::Router::new()
        .nest("/rpc", rpc)
        .nest("/rest", openapi)
        .layer(CorsLayer::permissive());

    let addr = "0.0.0.0:3000";
    println!("\nServer listening on http://{addr}");
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

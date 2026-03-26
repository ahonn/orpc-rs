use std::sync::{Arc, Mutex};

use orpc::*;
use serde::{Deserialize, Serialize};
use tower_http::cors::CorsLayer;

// --- Context ---

#[derive(Clone)]
struct AppCtx {
    db: Arc<Mutex<PlanetDb>>,
}

struct PlanetDb {
    planets: Vec<Planet>,
    next_id: u32,
}

impl PlanetDb {
    fn new() -> Self {
        let planets = vec![
            Planet { id: 1, name: "Mercury".into(), radius_km: 2440, has_rings: false },
            Planet { id: 2, name: "Venus".into(), radius_km: 6052, has_rings: false },
            Planet { id: 3, name: "Earth".into(), radius_km: 6371, has_rings: false },
            Planet { id: 4, name: "Mars".into(), radius_km: 3390, has_rings: false },
            Planet { id: 5, name: "Jupiter".into(), radius_km: 69911, has_rings: true },
            Planet { id: 6, name: "Saturn".into(), radius_km: 58232, has_rings: true },
            Planet { id: 7, name: "Uranus".into(), radius_km: 25362, has_rings: true },
            Planet { id: 8, name: "Neptune".into(), radius_km: 24622, has_rings: true },
        ];
        PlanetDb { next_id: 9, planets }
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
    Ok(planet)
}

// --- Router & Server ---

fn build_router() -> Router<AppCtx> {
    router! {
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
    }
}

#[tokio::main]
async fn main() {
    let db = Arc::new(Mutex::new(PlanetDb::new()));

    let router = build_router();
    println!("Procedures registered:");
    for key in router.procedures().keys() {
        println!("  POST /{key}");
    }

    let app = orpc_axum::into_router(router, move |_parts: &http::request::Parts| AppCtx {
        db: db.clone(),
    })
    .layer(CorsLayer::permissive());

    let addr = "0.0.0.0:3000";
    println!("\nServer listening on http://{addr}");
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

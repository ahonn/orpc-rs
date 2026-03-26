use std::sync::{Arc, Mutex};

use orpc::*;
use orpc_specta::{specta, Type};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

// --- Context ---

#[derive(Clone)]
struct AppCtx {
    db: Arc<Mutex<PlanetDb>>,
    planet_tx: broadcast::Sender<Planet>,
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

// --- Types (with specta::Type for TS generation) ---

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
struct Planet {
    id: u32,
    name: String,
    radius_km: u32,
    has_rings: bool,
}

#[derive(Debug, Serialize, Deserialize, Type)]
struct FindPlanetInput {
    name: String,
}

#[derive(Debug, Serialize, Deserialize, Type)]
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
    let _ = ctx.planet_tx.send(planet.clone());
    Ok(planet)
}

// --- Router ---

fn build_router() -> Router<AppCtx> {
    // SSE-like subscription: streams newly created planets via broadcast channel.
    // In Tauri, this is polled via repeated IPC calls (Tauri invoke is request-response).
    // For real-time push, a Tauri Event would be needed (future enhancement).
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
                .input(specta::<FindPlanetInput>())
                .handler(find_planet),
            "create" => os::<AppCtx>()
                .input(specta::<CreatePlanetInput>())
                .handler(create_planet),
        },
    };
    r._insert("planet.stream", planet_stream_proc);
    r
}

// --- App ---

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let db = Arc::new(Mutex::new(PlanetDb::new()));
    let (planet_tx, _) = broadcast::channel::<Planet>(16);
    let router = build_router();

    // Export TypeScript bindings at startup (dev only).
    #[cfg(debug_assertions)]
    {
        let bindings_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../src/bindings.ts");
        if let Err(e) = orpc_specta::export_ts(&router, bindings_path.to_str().unwrap()) {
            eprintln!("Failed to export TS bindings: {e}");
        } else {
            println!("TypeScript bindings exported to src/bindings.ts");
        }
    }

    let db_clone = db.clone();
    let tx_clone = planet_tx.clone();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_orpc::init(router, move |_app| AppCtx {
            db: db_clone.clone(),
            planet_tx: tx_clone.clone(),
        }))
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

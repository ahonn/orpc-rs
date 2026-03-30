use orpc::*;
use serde::{Deserialize, Serialize};

// --- Types ---

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct Planet {
    name: String,
    radius: u32,
}

#[derive(Debug, Serialize, Deserialize)]
struct FindInput {
    name: String,
}

// --- Service definition (single source of truth) ---

#[orpc_service(context = ())]
pub trait PlanetApi {
    async fn ping(&self, ctx: ()) -> Result<String, ORPCError>;
    async fn find_planet(&self, ctx: (), input: FindInput) -> Result<Planet, ORPCError>;
}

// --- Server implementation ---

struct PlanetApiImpl;

impl PlanetApi for PlanetApiImpl {
    async fn ping(&self, _ctx: ()) -> Result<String, ORPCError> {
        Ok("pong".into())
    }

    async fn find_planet(&self, _ctx: (), input: FindInput) -> Result<Planet, ORPCError> {
        match input.name.as_str() {
            "Earth" => Ok(Planet {
                name: "Earth".into(),
                radius: 6371,
            }),
            _ => Err(ORPCError::not_found(format!(
                "Planet '{}' not found",
                input.name
            ))),
        }
    }
}

// --- Tests ---

#[test]
fn router_has_correct_procedures() {
    let router = planet_api_router(PlanetApiImpl);
    assert!(router.get("ping").is_some(), "should have 'ping' procedure");
    assert!(
        router.get("find_planet").is_some(),
        "should have 'find_planet' procedure"
    );
}

#[tokio::test]
async fn router_ping_works() {
    let router = planet_api_router(PlanetApiImpl);
    let proc = router.get("ping").unwrap();
    let input = DynInput::from_value(serde_json::Value::Null);
    let mut stream = proc.exec((), input);

    use futures_util::StreamExt;
    let result = stream.next().await.unwrap().unwrap();
    let value: String = serde_json::from_value(result.to_value().unwrap()).unwrap();
    assert_eq!(value, "pong");
}

#[tokio::test]
async fn router_find_planet_works() {
    let router = planet_api_router(PlanetApiImpl);
    let proc = router.get("find_planet").unwrap();
    let input = DynInput::from_value(serde_json::json!({"name": "Earth"}));
    let mut stream = proc.exec((), input);

    use futures_util::StreamExt;
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

#[test]
fn client_struct_exists() {
    // Verify the client struct was generated with the correct name
    let _client = PlanetApiClient::new("http://localhost:3000/rpc");
}

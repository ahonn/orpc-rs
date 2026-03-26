use futures_util::StreamExt;
use orpc_procedure::{
    DynInput, DynOutput, ErasedProcedure, Meta, ProcedureError, ProcedureStream, Route, State,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
struct FindPlanetInput {
    name: String,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct Planet {
    name: String,
    radius: u32,
}

/// End-to-end: construct ErasedProcedure → exec with DynInput::Value → collect → verify.
#[tokio::test]
async fn end_to_end_procedure_execution() {
    let procedure = ErasedProcedure::new(
        |_ctx: (), input: DynInput| {
            let find: FindPlanetInput = input.deserialize().unwrap();
            ProcedureStream::from_future(async move {
                if find.name == "Earth" {
                    Ok(DynOutput::new(Planet {
                        name: "Earth".into(),
                        radius: 6371,
                    }))
                } else {
                    Err(ProcedureError::Resolver(Box::new(std::io::Error::new(
                        std::io::ErrorKind::NotFound,
                        format!("planet '{}' not found", find.name),
                    ))))
                }
            })
        },
        Route::get("/planets/{name}").tag("planets").summary("Find planet"),
        Meta::default(),
    );

    // Successful execution
    let input = DynInput::from_value(serde_json::json!({"name": "Earth"}));
    let mut stream = procedure.exec((), input);
    let result = stream.next().await.unwrap().unwrap();
    let planet: Planet = serde_json::from_value(result.to_value().unwrap()).unwrap();
    assert_eq!(
        planet,
        Planet {
            name: "Earth".into(),
            radius: 6371,
        }
    );
    assert!(stream.next().await.is_none());

    // Error execution
    let input = DynInput::from_value(serde_json::json!({"name": "Vulcan"}));
    let mut stream = procedure.exec((), input);
    let result = stream.next().await.unwrap();
    assert!(matches!(result, Err(ProcedureError::Resolver(_))));
}

/// Panic safety: handler that panics returns Unwind error instead of crashing.
#[tokio::test]
async fn panic_safety_integration() {
    let procedure = ErasedProcedure::new(
        |_ctx: (), _input: DynInput| -> ProcedureStream {
            panic!("unexpected crash in handler");
        },
        Route::default(),
        Meta::default(),
    );

    let input = DynInput::from_value(serde_json::json!(null));
    let mut stream = procedure.exec((), input);
    let result = stream.next().await.unwrap();
    assert!(matches!(result, Err(ProcedureError::Unwind(_))));
}

/// DynInput materialize then deserialize.
#[tokio::test]
async fn materialize_then_deserialize() {
    let input = DynInput::from_value(serde_json::json!({"name": "Mars"}));

    // Materialize (no-op for Value variant)
    let input = input.materialize().unwrap();

    // Inspect
    assert_eq!(
        input.as_value(),
        Some(&serde_json::json!({"name": "Mars"}))
    );

    // Deserialize
    let find: FindPlanetInput = input.deserialize().unwrap();
    assert_eq!(find.name, "Mars");
}

/// State container integration with procedure.
#[test]
fn state_container_usage() {
    let mut state = State::new();

    #[derive(Debug, PartialEq)]
    struct DbPool(String);

    state.insert(DbPool("postgres://localhost/test".into()));
    state.insert(42u32);

    assert_eq!(
        state.get::<DbPool>(),
        Some(&DbPool("postgres://localhost/test".into()))
    );
    assert_eq!(state.get::<u32>(), Some(&42));
}

/// Streaming procedure (multi-item output).
#[tokio::test]
async fn streaming_procedure() {
    let procedure = ErasedProcedure::new(
        |_ctx: (), _input: DynInput| {
            let items = vec![
                Ok(DynOutput::new(Planet {
                    name: "Mercury".into(),
                    radius: 2439,
                })),
                Ok(DynOutput::new(Planet {
                    name: "Venus".into(),
                    radius: 6051,
                })),
                Ok(DynOutput::new(Planet {
                    name: "Earth".into(),
                    radius: 6371,
                })),
            ];
            ProcedureStream::from_stream(futures_util::stream::iter(items))
        },
        Route::get("/planets"),
        Meta::default(),
    );

    let input = DynInput::from_value(serde_json::json!(null));
    let stream = procedure.exec((), input);
    let results: Vec<_> = stream.collect().await;
    assert_eq!(results.len(), 3);

    let names: Vec<String> = results
        .iter()
        .map(|r| {
            let v = r.as_ref().unwrap().to_value().unwrap();
            v["name"].as_str().unwrap().to_string()
        })
        .collect();
    assert_eq!(names, vec!["Mercury", "Venus", "Earth"]);
}

/// Compile-time assertions: all public types are Send.
#[test]
fn all_types_are_send() {
    fn assert_send<T: Send>() {}
    assert_send::<DynInput>();
    assert_send::<DynOutput>();
    assert_send::<ProcedureStream>();
    assert_send::<ProcedureError>();
    assert_send::<State>();
    assert_send::<ErasedProcedure<()>>();
    assert_send::<Route>();
    assert_send::<Meta>();
}

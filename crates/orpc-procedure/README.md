# orpc-procedure

Type-erased execution engine for oRPC procedures.

## Overview

This is the lowest-level crate in the orpc-rs stack. It defines the core abstractions that all other crates build upon:

- **`ErasedProcedure<TCtx>`** — A type-erased procedure that accepts dynamic input and produces a `ProcedureStream`
- **`ProcedureStream`** — Unified output type supporting both single-value (`from_future`) and streaming (`from_stream`) results
- **`DynInput` / `DynOutput`** — Type-erased wrappers for serde-compatible values
- **`ErasedSchema`** — Trait for type-erased input/output schemas (extended by `orpc-specta`)

## Usage

Most users should use the higher-level `orpc` crate instead. This crate is useful for:

- Building custom integrations (server adapters, client code generators)
- Working with procedures after type erasure
- Implementing custom schema adapters

```rust
use orpc_procedure::*;

let proc = ErasedProcedure::new(
    |ctx: MyCtx, input: DynInput| {
        ProcedureStream::from_future(async move {
            let name: String = input.deserialize()?;
            Ok(DynOutput::new(format!("Hello, {name}!")))
        })
    },
    Route::default(),
    Meta::default(),
);
```

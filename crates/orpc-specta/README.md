# orpc-specta

TypeScript type generation for oRPC routers via [specta](https://github.com/oscartbeaumont/specta).

## Overview

Generates TypeScript type definitions compatible with `@orpc/client`, enabling fully type-safe RPC from Rust to TypeScript.

## Generated Output

```typescript
import type { Client } from "@orpc/client"

export type Planet = { id: number; name: string; radius_km: number }
export type FindPlanetInput = { name: string }

export type Procedures = {
  ping: Client<Record<never, never>, void, string, Error>
  planet: {
    find: Client<Record<never, never>, FindPlanetInput, Planet, Error>
    list: Client<Record<never, never>, void, Planet[], Error>
  }
}
```

## Usage

```rust
use orpc_specta::{specta, Type};

#[derive(Serialize, Deserialize, Type)]
struct Planet { id: u32, name: String }

let router = router! {
    "planet" => {
        "find" => os::<AppCtx>()
            .input(specta::<FindInput>())
            .output(specta::<Planet>())
            .handler(find_planet),
    },
};

// Export to file (typically in debug builds)
orpc_specta::export_ts(&router, "../src/bindings.ts")?;
```

## TypeScript Integration

```typescript
import { createORPCClient } from "@orpc/client"
import { createTanstackQueryUtils } from "@orpc/tanstack-query"
import { TauriLink } from "@orpc-rs/tauri"
import type { Procedures } from "./bindings"

const client = createORPCClient<Procedures>(TauriLink())
const orpc = createTanstackQueryUtils(client)

// Fully typed queries and mutations
const { data } = useQuery(orpc.planet.find.queryOptions({ input: { name: "Earth" } }))
```

# @orpc-rs/tauri

TauriLink for `@orpc/client` — routes RPC calls through Tauri IPC instead of HTTP.

## Install

```bash
npm install @orpc-rs/tauri @orpc/client @tauri-apps/api
```

## Usage

```typescript
import { createORPCClient } from "@orpc/client"
import { TauriLink } from "@orpc-rs/tauri"
import type { Procedures } from "./bindings"

const client = createORPCClient<Procedures>(TauriLink())

// Query / Mutation
const planet = await client.planet.find({ name: "Earth" })

// Subscription (returns AsyncIterableIterator)
for await (const planet of await client.planet.stream()) {
  console.log("New planet:", planet)
}
```

## With TanStack Query

```typescript
import { createTanstackQueryUtils } from "@orpc/tanstack-query"
import { useQuery, useMutation } from "@tanstack/react-query"

const orpc = createTanstackQueryUtils(client)

// In components
const { data } = useQuery(orpc.planet.list.queryOptions({}))
const mutation = useMutation(orpc.planet.create.mutationOptions())
```

## Options

```typescript
TauriLink({
  pluginName: "orpc", // default, matches tauri-plugin-orpc
})
```

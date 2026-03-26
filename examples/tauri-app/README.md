# tauri-app example

Desktop app example: oRPC over Tauri IPC with full `@orpc/client` + TanStack Query integration.

## Features

- **Zero HTTP** — All RPC calls go through Tauri IPC via `TauriLink`
- **Type-safe** — Generated `Procedures` type from Rust via `orpc-specta`
- **TanStack Query** — `useQuery`, `useMutation` via `@orpc/tanstack-query`
- **Subscriptions** — Real-time planet stream via Tauri `Channel` → `AsyncIterableIterator`
- **Planet CRUD** — Same demo as axum-react, running entirely in-process

## Architecture

```
Rust Router ──orpc-specta──▶ bindings.ts (Client<> types)
     │                              │
     ▼                              ▼
tauri-plugin-orpc          createORPCClient<Procedures>(TauriLink())
     │                              │
     ▼                              ▼
  handle_rpc IPC ◀────────▶ TanStack Query hooks
```

## Run

```bash
cd examples/tauri-app
npm install
cargo tauri dev
```

TypeScript bindings are auto-generated on startup (debug builds).

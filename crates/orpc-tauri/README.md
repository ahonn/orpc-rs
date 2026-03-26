# tauri-plugin-orpc

Tauri v2 plugin for serving oRPC routers via IPC.

## Overview

Replaces HTTP transport with Tauri's native IPC, enabling zero-network-overhead RPC in desktop apps. Supports both request-response and streaming subscriptions via Tauri's `Channel` API.

## Rust Setup

```rust
// src-tauri/src/lib.rs
tauri::Builder::default()
    .plugin(tauri_plugin_orpc::init(router, |app_handle| AppCtx {
        db: db.clone(),
    }))
    .run(tauri::generate_context!())
    .unwrap();
```

## How It Works

Registers a single IPC command `plugin:orpc|handle_rpc` that auto-detects procedure type:

- **Single-value** (query/mutation): Returns JSON response directly
- **Subscription** (stream): Spawns a background task, streams events via `Channel`, returns immediately

## Permissions

Add `"orpc:default"` to your Tauri capabilities:

```json
{
  "permissions": ["core:default", "orpc:default"]
}
```

## TypeScript Side

Pair with [`@orpc-rs/tauri`](../../packages/tauri) for the `TauriLink` client.

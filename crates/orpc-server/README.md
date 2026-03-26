# orpc-server

Wire protocol implementation for oRPC, compatible with the official `@orpc/client` TypeScript package.

## Protocols

### RPC Protocol

Request-response and SSE streaming over a single endpoint, matching `@orpc/client`'s `RPCLink`.

- **Request**: `{"json": <input>}` or `{}` (no input), with optional `"meta"` for special types
- **Response**: `{"json": <output>}` (meta omitted when empty)
- **SSE**: Auto-detected via `ProcedureStream::size_hint()` — single-value returns JSON, multi-value streams SSE events (`message`, `done`, `error`)

### OpenAPI Protocol

REST-style routing with `Route` metadata, path parameter extraction, and plain JSON responses (no envelope).

## Modules

- `rpc` — RPC encode/decode, `execute_rpc`, `execute_rpc_auto`
- `sse` — SSE event formatting, `SseStream`, subscription detection
- `openapi` — Route matching, OpenAPI request/response handling
- `meta` — Meta array parsing for special type reconstruction (Date, Set, Map, etc.)

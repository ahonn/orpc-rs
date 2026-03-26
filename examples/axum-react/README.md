# axum-react example

Full-stack web example: Rust oRPC server (axum) + React client.

## Features

- **RPC endpoint** (`/rpc`) — Query, mutation, SSE subscription via `@orpc/client` `RPCLink`
- **OpenAPI endpoint** (`/rest`) — REST-style `GET /planets/{id}`, `POST /planets`
- **Planet CRUD** — List, find, create planets with shared in-memory state
- **Real-time** — SSE subscription streams newly created planets

## Run

```bash
# Terminal 1: Rust server
cd examples/axum-react/server
cargo run

# Terminal 2: React client
cd examples/axum-react/client
npm install && npm run dev
```

Server runs on `http://localhost:3000`, client on `http://localhost:5173` (Vite proxies `/rpc` and `/rest`).

import { createORPCClient } from "@orpc/client";
import { RPCLink } from "@orpc/client/fetch";
import { createTanstackQueryUtils } from "@orpc/tanstack-query";

// --- Types matching the Rust server ---

export interface Planet {
  id: number;
  name: string;
  radius_km: number;
  has_rings: boolean;
}

export interface FindPlanetInput {
  name: string;
}

export interface CreatePlanetInput {
  name: string;
  radius_km: number;
  has_rings: boolean;
}

export interface UploadInput {
  description: string;
  file: Blob;
}

export interface UploadResult {
  filename: string;
  size: number;
  content_type: string;
  description: string;
}

// --- oRPC client setup (RPC protocol) ---

// RPCLink requires an absolute URL.
// Vite proxies /rpc → http://localhost:3000/rpc in development.
const link = new RPCLink({
  url: () => `${window.location.origin}/rpc`,
});

// The untyped client — procedure paths are built dynamically via Proxy.
// client.planet.find(input) → POST /rpc/planet/find {"json": input}
export const client: any = createORPCClient(link);

// TanStack Query integration — generates queryOptions / mutationOptions.
export const orpc: any = createTanstackQueryUtils(client);

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

// --- oRPC client setup ---

// In development, Vite proxies /api → http://localhost:3000.
// In production, point directly to the Rust server.
// RPCLink requires an absolute URL, so we prepend the current origin.
const link = new RPCLink({
  url: () => `${window.location.origin}/api`,
});

// The untyped client — procedure paths are built dynamically via Proxy.
// client.planet.find(input) → POST /api/planet/find {"json": input, "meta": []}
export const client: any = createORPCClient(link);

// TanStack Query integration — generates queryOptions / mutationOptions.
export const orpc: any = createTanstackQueryUtils(client);

import { createORPCClient } from "@orpc/client";
import { createTanstackQueryUtils } from "@orpc/tanstack-query";
import { QueryClient } from "@tanstack/react-query";
import { TauriLink } from "@orpc-rs/tauri";
import type { Procedures } from "../bindings";

export type { Planet, FindPlanetInput, CreatePlanetInput } from "../bindings";

const link = TauriLink();

export const client = createORPCClient<Procedures>(link);

export const orpc = createTanstackQueryUtils(client);

export const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: 1000 * 60,
    },
  },
});

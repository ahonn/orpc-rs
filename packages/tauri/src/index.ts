import { invoke } from "@tauri-apps/api/core";

export interface TauriLinkOptions {
  /** Tauri plugin name. Default: "orpc" */
  pluginName?: string;
}

interface RpcResponse {
  status: number;
  body: { json?: unknown; meta?: unknown[] };
}

/**
 * Create a link for `@orpc/client` that routes RPC calls through Tauri IPC.
 *
 * Replaces `RPCLink` — no HTTP server needed in desktop apps.
 *
 * @example
 * ```ts
 * import { createORPCClient } from '@orpc/client'
 * import { TauriLink } from '@orpc-rs/tauri'
 *
 * const link = TauriLink()
 * const client = createORPCClient(link)
 * const planet = await client.planet.find({ name: 'Earth' })
 * ```
 */
export function TauriLink(options?: TauriLinkOptions) {
  const pluginName = options?.pluginName ?? "orpc";
  const command = `plugin:${pluginName}|handle_rpc_call`;

  return {
    call: async (
      path: readonly string[],
      input: unknown,
      _options?: unknown,
    ): Promise<unknown> => {
      const request = {
        path: path.join("."),
        input: input !== undefined ? { json: input } : {},
      };

      const response = await invoke<RpcResponse>(command, { request });

      if (response.status >= 400) {
        const errorData = response.body?.json as Record<string, unknown> | undefined;
        const error = new Error(
          (errorData?.message as string) ?? "RPC error",
        );
        Object.assign(error, errorData);
        throw error;
      }

      return response.body?.json;
    },
  };
}

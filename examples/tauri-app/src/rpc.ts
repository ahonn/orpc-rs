import { invoke, Channel } from "@tauri-apps/api/core";

interface RpcResponse {
  status: number;
  body: { json?: unknown };
}

interface SubscriptionEvent {
  event: "message" | "done" | "error";
  id?: number;
  data?: { json?: unknown; code?: string; message?: string };
}

export async function rpcCall<T>(path: string, input?: unknown): Promise<T> {
  const request = {
    path,
    input: input !== undefined ? { json: input } : {},
  };

  const response = await invoke<RpcResponse>("plugin:orpc|handle_rpc_call", {
    request,
  });

  if (response.status >= 400) {
    const err = response.body?.json as Record<string, unknown> | undefined;
    throw new Error(
      (err?.message as string) ?? `RPC error (${response.status})`,
    );
  }

  return response.body?.json as T;
}

/**
 * Subscribe to a streaming procedure via Tauri Channel.
 *
 * Returns an unsubscribe function.
 */
export function rpcSubscribe<T>(
  path: string,
  callbacks: {
    onMessage: (data: T, id: number) => void;
    onDone?: () => void;
    onError?: (error: unknown) => void;
  },
  input?: unknown,
): void {
  const request = {
    path,
    input: input !== undefined ? { json: input } : {},
  };

  const channel = new Channel<SubscriptionEvent>();
  channel.onmessage = (event) => {
    switch (event.event) {
      case "message":
        callbacks.onMessage(event.data?.json as T, event.id ?? 0);
        break;
      case "done":
        callbacks.onDone?.();
        break;
      case "error":
        callbacks.onError?.(event.data);
        break;
    }
  };

  invoke("plugin:orpc|handle_rpc_subscription", { request, channel }).catch(
    (err) => callbacks.onError?.(err),
  );
}

import { invoke, Channel } from "@tauri-apps/api/core";

export interface TauriLinkOptions {
  /** Tauri plugin name. Default: "orpc" */
  pluginName?: string;
}

interface SubscriptionEvent {
  event: "message" | "done" | "error";
  id?: number;
  data?: { json?: unknown; code?: string; message?: string };
}

interface UnifiedResponse {
  type: "response" | "subscription";
  status?: number;
  body?: { json?: unknown; meta?: unknown[] };
}

/**
 * Create a link for `@orpc/client` that routes RPC calls through Tauri IPC.
 *
 * Replaces `RPCLink` — no HTTP server needed in desktop apps.
 * Supports both request-response and subscription procedures.
 *
 * @example
 * ```ts
 * import { createORPCClient } from '@orpc/client'
 * import { TauriLink } from '@orpc-rs/tauri'
 *
 * const link = TauriLink()
 * const client = createORPCClient(link)
 *
 * // Query/mutation
 * const planet = await client.planet.find({ name: 'Earth' })
 *
 * // Subscription (returns AsyncIterableIterator)
 * for await (const planet of client.planet.stream()) {
 *   console.log('New planet:', planet)
 * }
 * ```
 */
export function TauriLink(options?: TauriLinkOptions) {
  const pluginName = options?.pluginName ?? "orpc";
  const command = `plugin:${pluginName}|handle_rpc`;

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

      const channel = new Channel<SubscriptionEvent>();
      const response = await invoke<UnifiedResponse>(command, {
        request,
        channel,
      });

      if (response.type === "subscription") {
        return channelToAsyncIterator(channel);
      }

      if (response.status! >= 400) {
        const errorData = response.body?.json as
          | Record<string, unknown>
          | undefined;
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

/**
 * Convert a Tauri Channel receiving subscription events into an
 * AsyncIterableIterator, matching @orpc/client's event iterator contract.
 */
function channelToAsyncIterator<T>(
  channel: Channel<SubscriptionEvent>,
): AsyncIterableIterator<T> {
  type QueueItem =
    | { type: "value"; value: T }
    | { type: "done" }
    | { type: "error"; error: unknown };

  const queue: QueueItem[] = [];
  let waiting: ((item: QueueItem) => void) | null = null;
  let finished = false;

  function push(item: QueueItem) {
    if (waiting) {
      const resolve = waiting;
      waiting = null;
      resolve(item);
    } else {
      queue.push(item);
    }
  }

  channel.onmessage = (event) => {
    switch (event.event) {
      case "message":
        push({ type: "value", value: event.data?.json as T });
        break;
      case "done":
        finished = true;
        push({ type: "done" });
        break;
      case "error":
        finished = true;
        push({ type: "error", error: event.data });
        break;
    }
  };

  function dequeue(): Promise<QueueItem> {
    if (queue.length > 0) {
      return Promise.resolve(queue.shift()!);
    }
    if (finished) {
      return Promise.resolve({ type: "done" });
    }
    return new Promise<QueueItem>((resolve) => {
      waiting = resolve;
    });
  }

  const iterator: AsyncIterableIterator<T> = {
    async next(): Promise<IteratorResult<T>> {
      const item = await dequeue();
      if (item.type === "value") return { value: item.value, done: false };
      if (item.type === "error") throw item.error;
      return { value: undefined as unknown as T, done: true };
    },
    [Symbol.asyncIterator]() {
      return this;
    },
  };

  return iterator;
}

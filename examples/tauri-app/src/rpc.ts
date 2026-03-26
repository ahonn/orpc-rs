import { TauriLink } from "@orpc-rs/tauri";

const link = TauriLink();

/**
 * Call an oRPC procedure via Tauri IPC.
 * Uses TauriLink from @orpc-rs/tauri under the hood.
 */
export async function rpcCall<T>(path: string, input?: unknown): Promise<T> {
  return link.call(path.split("."), input) as Promise<T>;
}

/**
 * Subscribe to a streaming procedure via Tauri IPC.
 *
 * Internally uses TauriLink which returns an AsyncIterableIterator
 * for subscription procedures. This wrapper provides a callback API
 * for compatibility with React components.
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
  (async () => {
    try {
      const iter = (await link.call(
        path.split("."),
        input,
      )) as AsyncIterableIterator<T>;
      let id = 0;
      for await (const item of iter) {
        callbacks.onMessage(item, id++);
      }
      callbacks.onDone?.();
    } catch (err) {
      callbacks.onError?.(err);
    }
  })();
}

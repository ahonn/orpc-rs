import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  server: {
    port: 5173,
    proxy: {
      // RPC protocol — used by @orpc/client RPCLink
      "/rpc": {
        target: "http://localhost:3000",
      },
      // OpenAPI protocol — REST-style endpoints
      "/rest": {
        target: "http://localhost:3000",
      },
    },
  },
});

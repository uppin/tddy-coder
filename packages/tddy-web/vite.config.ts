import path from "node:path";
import { fileURLToPath } from "node:url";

import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

const __dirname = path.dirname(fileURLToPath(import.meta.url));

export default defineConfig({
  plugins: [
    react({ babel: { plugins: ["babel-plugin-react-compiler"] } }),
  ],
  resolve: {
    // Cypress / dev server resolve `tddy-livekit-web` without requiring a prior `dist` build.
    alias: {
      "tddy-livekit-web": path.resolve(__dirname, "../tddy-livekit-web/src/index.ts"),
    },
  },
  server: {
    proxy: {
      "/rpc": {
        target: `http://127.0.0.1:${process.env.DAEMON_PORT ?? 8899}`,
        changeOrigin: true,
      },
      "/api": {
        target: `http://127.0.0.1:${process.env.DAEMON_PORT ?? 8899}`,
        changeOrigin: true,
      },
    },
  },
});

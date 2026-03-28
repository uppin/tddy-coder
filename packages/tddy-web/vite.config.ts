import path from "node:path";
import { fileURLToPath } from "node:url";
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

const __dirname = path.dirname(fileURLToPath(import.meta.url));

export default defineConfig({
  plugins: [
    react({ babel: { plugins: ["babel-plugin-react-compiler"] } }),
  ],
  server: {
    fs: {
      // Allow Cypress CT and monorepo imports to resolve specs outside the package root.
      allow: [path.resolve(__dirname, "../..")],
    },
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

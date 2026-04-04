import path from "node:path";
import { fileURLToPath } from "node:url";
import tailwindcss from "@tailwindcss/vite";
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

const __dirname = path.dirname(fileURLToPath(import.meta.url));

/** Cypress component tests rely on imperative refs and predictable state; React Compiler can break those paths. */
const useReactCompiler = process.env.CYPRESS_DISABLE_REACT_COMPILER !== "1";

export default defineConfig({
  plugins: [
    tailwindcss(),
    react(
      useReactCompiler
        ? { babel: { plugins: ["babel-plugin-react-compiler"] } }
        : {}
    ),
  ],
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "./src"),
      // Cypress / dev server resolve `tddy-livekit-web` without requiring a prior `dist` build.
      "tddy-livekit-web": path.resolve(__dirname, "../tddy-livekit-web/src/index.ts"),
    },
  },
  build: {
    outDir: "dist",
    emptyOutDir: true,
  },
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

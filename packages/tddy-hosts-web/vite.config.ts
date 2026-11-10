import path from "node:path";
import { fileURLToPath } from "node:url";
import tailwindcss from "@tailwindcss/vite";
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { prebundledDeps } from "../../scripts/vite-prebundle";

const __dirname = path.dirname(fileURLToPath(import.meta.url));

/** Same switch as tddy-web: component tests run uncompiled (docs/dev/todo/2026-10-23-cypress-runs-without-the-react-compiler.md). */
const useReactCompiler = process.env.CYPRESS_DISABLE_REACT_COMPILER !== "1";

export default defineConfig({
  // Pre-bundle up front so Cypress never meets a re-optimise reload mid-run (scripts/vite-prebundle.ts).
  optimizeDeps: prebundledDeps(__dirname),
  plugins: [
    tailwindcss(),
    react(useReactCompiler ? { babel: { plugins: ["babel-plugin-react-compiler"] } } : {}),
  ],
  resolve: {
    dedupe: ["react", "react-dom"],
  },
  server: {
    fs: { allow: [path.resolve(__dirname, "../..")] },
  },
});

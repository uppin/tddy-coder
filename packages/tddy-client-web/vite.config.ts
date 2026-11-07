import path from "node:path";
import { fileURLToPath } from "node:url";
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

const __dirname = path.dirname(fileURLToPath(import.meta.url));

/** Same switch as tddy-web: component tests run uncompiled (docs/dev/todo/2026-10-23-cypress-runs-without-the-react-compiler.md). */
const useReactCompiler = process.env.CYPRESS_DISABLE_REACT_COMPILER !== "1";

export default defineConfig({
  plugins: [
    react(useReactCompiler ? { babel: { plugins: ["babel-plugin-react-compiler"] } } : {}),
  ],
  resolve: {
    dedupe: ["react", "react-dom"],
  },
  server: {
    fs: { allow: [path.resolve(__dirname, "../..")] },
  },
});

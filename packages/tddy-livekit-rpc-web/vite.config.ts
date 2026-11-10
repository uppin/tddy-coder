import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { prebundledDeps } from "../../scripts/vite-prebundle";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));

export default defineConfig({
  // Pre-bundle up front so Cypress never meets a re-optimise reload mid-run (scripts/vite-prebundle.ts).
  optimizeDeps: prebundledDeps(__dirname),
  plugins: [react()],
});

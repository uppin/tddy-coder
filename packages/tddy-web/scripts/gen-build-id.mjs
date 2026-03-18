#!/usr/bin/env node
/**
 * Generates buildId.ts with a unique build identifier (timestamp).
 * Run before build so mobile users can verify they have the latest version.
 */
import { writeFileSync } from "fs";
import { dirname, join } from "path";
import { fileURLToPath } from "url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const outPath = join(__dirname, "..", "src", "buildId.ts");
const buildId = new Date().toISOString().replace(/[:.]/g, "-").slice(0, 19);

writeFileSync(
  outPath,
  `// Auto-generated at build time by scripts/gen-build-id.mjs
export const BUILD_ID = "${buildId}";
`,
  "utf8"
);

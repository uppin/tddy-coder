/**
 * The `optimizeDeps` every web package's `vite.config.ts` uses, so Vite pre-bundles its dependencies
 * once at dev-server start instead of discovering one mid-run.
 *
 * Under Cypress component testing a dependency Vite meets for the first time while a spec is loading
 * makes it re-optimise and reload, and the spec in flight fails with "Failed to fetch dynamically
 * imported module …/cypress/support/component.ts" — a red shard that a re-run turns green. Scanning
 * the specs, the support files and the sources up front, and naming the heavy dependencies
 * explicitly, leaves nothing to discover.
 */

import { existsSync } from "node:fs";
import { createRequire } from "node:module";
import { join } from "node:path";

/** Large or deeply imported dependencies; each is pre-bundled where the package can resolve it. */
export const PREBUNDLED = [
  "react",
  "react-dom",
  "react-dom/client",
  "react/jsx-runtime",
  "react/jsx-dev-runtime",
  "@bufbuild/protobuf",
  "@bufbuild/protobuf/wire",
  "@bufbuild/protobuf/codegenv2",
  "@connectrpc/connect",
  "@connectrpc/connect-web",
  "livekit-client",
  "lucide-react",
  "rxjs",
  "dequal",
  "debug",
  "ghostty-web",
  "@radix-ui/react-select",
  "@radix-ui/react-scroll-area",
  "@radix-ui/react-tooltip",
  "@radix-ui/react-slot",
  "class-variance-authority",
  "clsx",
  "tailwind-merge",
  "cypress/react",
] as const;

/** `optimizeDeps` for the package rooted at `packageDir`. */
export function prebundledDeps(packageDir: string): { entries: string[]; include: string[] } {
  const require = createRequire(join(packageDir, "package.json"));
  const resolvable = (name: string) => {
    try {
      require.resolve(name);
      return true;
    } catch {
      return false;
    }
  };
  return {
    entries: [
      ...(existsSync(join(packageDir, "index.html")) ? ["index.html"] : []),
      "src/**/*.{ts,tsx}",
      "cypress/component/**/*.cy.{ts,tsx}",
      "cypress/support/**/*.{ts,tsx}",
    ],
    include: PREBUNDLED.filter(resolvable),
  };
}

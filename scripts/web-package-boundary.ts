/**
 * Package-boundary rule for the web packages: which modules a package's sources import.
 *
 * Each package's `src/packageBoundary.test.ts` lists what it must never import (`tddy-web`,
 * `livekit-client`, generated code, …) and asserts `forbiddenImports(...)` is empty.
 */
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, relative } from "node:path";

/** A forbidden import found in a package's sources. */
export interface ForbiddenImport {
  /** File path relative to the package directory. */
  readonly file: string;
  /** The module specifier as written. */
  readonly specifier: string;
}

/** A specifier is forbidden when it equals a rule, starts with `<rule>/`, or matches a RegExp rule. */
export type ImportRule = string | RegExp;

/** Every import in `<packageDir>/<srcDir>` (recursively, `.ts`/`.tsx`) that matches a rule. */
export function forbiddenImports(
  packageDir: string,
  rules: readonly ImportRule[],
  srcDir = "src",
): ForbiddenImport[] {
  const root = join(packageDir, srcDir);
  const found: ForbiddenImport[] = [];
  for (const path of sourceFiles(root)) {
    for (const specifier of importSpecifiers(readFileSync(path, "utf8"))) {
      if (rules.some((rule) => matches(rule, specifier))) {
        found.push({ file: relative(packageDir, path), specifier });
      }
    }
  }
  return found;
}

function matches(rule: ImportRule, specifier: string): boolean {
  if (typeof rule === "string") return specifier === rule || specifier.startsWith(`${rule}/`);
  return rule.test(specifier);
}

function sourceFiles(dir: string): string[] {
  const out: string[] = [];
  for (const entry of readdirSync(dir).sort()) {
    const path = join(dir, entry);
    if (statSync(path).isDirectory()) out.push(...sourceFiles(path));
    else if (/\.(ts|tsx)$/.test(entry)) out.push(path);
  }
  return out;
}

/** `import`/`export ... from "x"`, side-effect `import "x"`, and `import("x")`. */
const IMPORT_FORMS = [
  /\b(?:import|export)\s[^"'`;]*?\bfrom\s*["']([^"']+)["']/g,
  /\bimport\s*["']([^"']+)["']/g,
  /\bimport\s*\(\s*["']([^"']+)["']\s*\)/g,
];

function importSpecifiers(source: string): string[] {
  return IMPORT_FORMS.flatMap((form) => [...source.matchAll(form)].map((m) => m[1]));
}

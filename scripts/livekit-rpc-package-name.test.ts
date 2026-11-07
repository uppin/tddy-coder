import { describe, expect, test } from "bun:test";
import { execFileSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";

/**
 * The LiveKit RPC transport is `tddy-livekit-rpc-web`. The name `tddy-livekit-web` is handed to the
 * LiveKit wiring package later in the stack (`#web-split` 6/16), so these rules pin the *transport's*
 * identity rather than banning the old name: nothing reaches a transport export through
 * `tddy-livekit-web`, and whatever package holds that name is not the transport.
 */

const REPO_ROOT = join(import.meta.dir, "..");
const TRANSPORT_DIR = join(REPO_ROOT, "packages/tddy-livekit-rpc-web");
const TRANSPORT_SOURCES = ["transport.ts", "transport-factory.ts", "chunking.ts"];

/** Names the transport package exports from its entry point. */
function transportExports(): Set<string> {
  const entry = readFileSync(join(TRANSPORT_DIR, "src/index.ts"), "utf8");
  const names = new Set<string>();
  for (const block of entry.matchAll(/export\s+(?:type\s+)?\{([^}]*)\}/g)) {
    for (const part of block[1].split(",")) {
      const name = part.trim().split(/\s+as\s+/).pop()?.replace(/^type\s+/, "").trim();
      if (name) names.add(name);
    }
  }
  return names;
}

/** `[file, imported names]` for every import of `tddy-livekit-web` in tracked web sources and specs. */
function importsOfTheOldName(): Array<[string, string[]]> {
  let files: string[] = [];
  try {
    files = execFileSync(
      "git",
      ["grep", "-l", "-E", "from ['\"]tddy-livekit-web['\"]", "--", "packages/*/src/**", "packages/*/cypress/**"],
      { cwd: REPO_ROOT, encoding: "utf8" },
    )
      .split("\n")
      .filter(Boolean);
  } catch {
    return []; // git grep exits 1 when nothing matches
  }
  return files.map((file) => {
    const body = readFileSync(join(REPO_ROOT, file), "utf8");
    const names = [...body.matchAll(/import\s+(?:type\s+)?\{([^}]*)\}\s+from\s+['"]tddy-livekit-web['"]/g)]
      .flatMap((m) => m[1].split(","))
      .map((n) => n.trim().replace(/^type\s+/, "").split(/\s+as\s+/)[0].trim())
      .filter(Boolean);
    return [file, names];
  });
}

describe("the LiveKit RPC transport package", () => {
  test("is named tddy-livekit-rpc-web and has its own web workflow", () => {
    // Given
    const manifest = join(TRANSPORT_DIR, "package.json");

    // When
    const name = existsSync(manifest) ? JSON.parse(readFileSync(manifest, "utf8")).name : null;

    // Then
    expect(name).toBe("tddy-livekit-rpc-web");
    expect(existsSync(join(REPO_ROOT, ".github/workflows/web-tddy-livekit-rpc-web.yml"))).toBe(true);
  });

  test("no file reaches a transport export through tddy-livekit-web", () => {
    // Given
    const exported = transportExports();

    // When
    const offenders = importsOfTheOldName()
      .map(([file, names]) => [file, names.filter((n) => exported.has(n))] as const)
      .filter(([, names]) => names.length > 0);

    // Then
    expect(offenders).toEqual([]);
  });

  test("whatever package is named tddy-livekit-web is not the transport", () => {
    // Given
    const oldDir = join(REPO_ROOT, "packages/tddy-livekit-web/src");

    // When
    const transportFilesLeft = TRANSPORT_SOURCES.filter((f) => existsSync(join(oldDir, f)));

    // Then
    expect(transportFilesLeft).toEqual([]);
  });

  test("the codegen manifest generates the transport's code at its new path", () => {
    // Given
    const manifest = readFileSync(join(REPO_ROOT, "scripts/generated-code.manifest"), "utf8");

    // When
    const rows = manifest.split("\n").filter((l) => /^packages\/tddy-livekit(-rpc)?-web\s/.test(l));

    // Then
    expect(rows.length).toBeGreaterThan(0);
    expect(rows.every((r) => r.startsWith("packages/tddy-livekit-rpc-web "))).toBe(true);
  });
});

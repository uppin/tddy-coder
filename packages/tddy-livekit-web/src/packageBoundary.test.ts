import { describe, expect, test } from "bun:test";
import { join } from "node:path";
import { forbiddenImports } from "../../../scripts/web-package-boundary";

const PACKAGE_DIR = join(import.meta.dir, "..");

describe("tddy-livekit-web package boundary", () => {
  test("imports nothing from tddy-web (it may import livekit-client: it is the LiveKit package)", () => {
    // When
    const found = forbiddenImports(PACKAGE_DIR, ["tddy-web", /\/tddy-web\//]);

    // Then
    expect(found).toEqual([]);
  });
});

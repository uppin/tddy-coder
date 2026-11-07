import { describe, expect, test } from "bun:test";
import { join } from "node:path";
import { forbiddenImports } from "../../../scripts/web-package-boundary";

const PACKAGE_DIR = join(import.meta.dir, "..");

describe("tddy-client-web package boundary", () => {
  test("imports nothing from tddy-web or livekit-client", () => {
    // When
    const found = forbiddenImports(PACKAGE_DIR, ["tddy-web", /\/tddy-web\//, "livekit-client", "tddy-livekit-web", "tddy-livekit-rpc-web"]);

    // Then
    expect(found).toEqual([]);
  });
});

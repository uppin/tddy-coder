import { describe, expect, test } from "bun:test";
import { join } from "node:path";
import { forbiddenImports } from "../../../scripts/web-package-boundary";

const PACKAGE_DIR = join(import.meta.dir, "..");

describe("tddy-ui-web package boundary", () => {
  test("imports nothing from tddy-web, generated code, connect or livekit", () => {
    // Given the modules every app concern lives behind
    const rules = [
      "tddy-web",
      /(^|\/)gen\//,
      /\/tddy-web\//,
      "@connectrpc/connect",
      "@bufbuild/protobuf",
      "livekit-client",
      "tddy-livekit-web",
      "tddy-livekit-rpc-web",
      "tddy-client-web",
    ];

    // When
    const found = forbiddenImports(PACKAGE_DIR, rules);

    // Then
    expect(found).toEqual([]);
  });

  test("owns the primitives, the theme and the shared hooks", () => {
    // Given
    const exportsOf = Object.keys(require("../src/index.ts"));

    // Then
    expect(exportsOf).toEqual(
      expect.arrayContaining([
        "Button",
        "InlineError",
        "ScrollArea",
        "Select",
        "Table",
        "Toast",
        "Tooltip",
        "cn",
        "useIsMobile",
        "useVisualViewport",
      ]),
    );
  });
});

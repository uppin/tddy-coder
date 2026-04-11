import { describe, expect, test } from "bun:test";

import { parseCodexOAuthMetadata } from "./codexOauthMetadata";

describe("parseCodexOAuthMetadata", () => {
  test("returns null for empty", () => {
    expect(parseCodexOAuthMetadata("")).toBeNull();
    expect(parseCodexOAuthMetadata("   ")).toBeNull();
  });

  test("parses pending authorize flow", () => {
    const j = JSON.stringify({
      codex_oauth: {
        pending: true,
        authorize_url: "https://auth.openai.com/x",
        callback_port: 8765,
        state: "s1",
      },
    });
    const p = parseCodexOAuthMetadata(j);
    expect(p).not.toBeNull();
    expect(p!.pending).toBe(true);
    expect(p!.authorizeUrl).toBe("https://auth.openai.com/x");
    expect(p!.callbackPort).toBe(8765);
    expect(p!.state).toBe("s1");
  });

  test("returns null for invalid JSON", () => {
    expect(parseCodexOAuthMetadata("{")).toBeNull();
  });
});

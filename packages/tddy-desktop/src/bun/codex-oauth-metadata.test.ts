import { describe, expect, test } from "bun:test";

import { parseCodexOAuthMetadata } from "./codex-oauth-metadata";

describe("parseCodexOAuthMetadata", () => {
  test("parses object", () => {
    const m = JSON.stringify({
      codex_oauth: { pending: true, authorize_url: "https://auth.openai.com/x", callback_port: 1 },
    });
    const p = parseCodexOAuthMetadata(m);
    expect(p?.pending).toBe(true);
    expect(p?.authorizeUrl).toContain("openai");
    expect(p?.callbackPort).toBe(1);
  });
});

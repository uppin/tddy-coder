import { describe, expect, test } from "bun:test";

import {
  DEFAULT_CODEX_OAUTH_CALLBACK_PORT,
  parseCodexOAuthMetadata,
  resolvedCodexOAuthCallbackPort,
} from "./codex-oauth-metadata";

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

describe("resolvedCodexOAuthCallbackPort", () => {
  test("uses default when missing or zero", () => {
    expect(resolvedCodexOAuthCallbackPort(null)).toBe(DEFAULT_CODEX_OAUTH_CALLBACK_PORT);
    expect(resolvedCodexOAuthCallbackPort({})).toBe(DEFAULT_CODEX_OAUTH_CALLBACK_PORT);
    expect(resolvedCodexOAuthCallbackPort({ callbackPort: 0 })).toBe(
      DEFAULT_CODEX_OAUTH_CALLBACK_PORT,
    );
  });

  test("accepts valid port", () => {
    expect(resolvedCodexOAuthCallbackPort({ callbackPort: 8765 })).toBe(8765);
  });
});

/**
 * Automated acceptance smoke for desktop phases 2–3 (metadata + callback listener).
 * Full GUI / LiveKit E2E is manual: `bun run --filter tddy-desktop dev` per package README.
 */
import { describe, expect, test } from "bun:test";
import { parseCodexOAuthMetadata } from "../../src/bun/codex-oauth-metadata";
import { startOAuthCallbackServer } from "../../src/bun/oauth-callback-server";

describe("tddy-desktop phases (acceptance)", () => {
  test("phase 2: codex_oauth pending metadata parses", () => {
    const json = JSON.stringify({
      codex_oauth: {
        pending: true,
        authorize_url:
          "https://auth.openai.com/authorize?client_id=x&response_type=code&scope=openid&state=abc&redirect_uri=http%3A%2F%2F127.0.0.1%3A14556%2Fauth%2Fcallback",
        state: "abc",
        callback_port: 14556,
      },
    });
    const parsed = parseCodexOAuthMetadata(json);
    expect(parsed?.pending).toBe(true);
    expect(parsed?.callbackPort).toBe(14556);
    expect(parsed?.state).toBe("abc");
  });

  test("phase 2: OAuth callback server accepts matching GET", async () => {
    let hit: { code: string; state: string } | null = null;
    const server = startOAuthCallbackServer({
      port: 0,
      expectedState: "s1",
      onHit: (h) => {
        hit = h;
      },
    });
    try {
      const port = server.port;
      const res = await fetch(
        `http://127.0.0.1:${port}/auth/callback?code=c1&state=s1`,
      );
      expect(res.ok).toBe(true);
      expect(hit).toEqual({ code: "c1", state: "s1" });
    } finally {
      server.stop();
    }
  });
});

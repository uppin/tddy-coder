import { describe, expect, test } from "bun:test";

import { startOAuthCallbackServer } from "./oauth-callback-server";

describe("startOAuthCallbackServer", () => {
  test("accepts matching callback", async () => {
    let hit: { code: string; state: string } | null = null;
    const server = startOAuthCallbackServer({
      port: 0,
      expectedState: "st",
      onHit: (h) => {
        hit = h;
      },
    });
    const port = server.port;
    const res = await fetch(
      `http://127.0.0.1:${port}/auth/callback?code=abc&state=st`,
    );
    expect(res.ok).toBe(true);
    expect(hit).toEqual({ code: "abc", state: "st" });
    server.stop();
  });

  test("rejects state mismatch when expected set", async () => {
    const server = startOAuthCallbackServer({
      port: 0,
      expectedState: "want",
      onHit: () => {},
    });
    const port = server.port;
    const res = await fetch(
      `http://127.0.0.1:${port}/auth/callback?code=a&state=other`,
    );
    expect(res.status).toBe(403);
    server.stop();
  });
});

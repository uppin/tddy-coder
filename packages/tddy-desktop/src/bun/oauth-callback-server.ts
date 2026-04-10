import type { Server } from "bun";

export type CallbackHit = { code: string; state: string };

/** Minimal GET /auth/callback handler; does not log query secrets. */
export function startOAuthCallbackServer(options: {
  port: number;
  expectedState: string;
  onHit: (hit: CallbackHit) => void;
}): Server {
  const { port, expectedState, onHit } = options;
  return Bun.serve({
    port,
    fetch(req) {
      const u = new URL(req.url);
      if (u.pathname !== "/auth/callback") {
        return new Response("Not Found", { status: 404 });
      }
      const code = u.searchParams.get("code") ?? "";
      const state = u.searchParams.get("state") ?? "";
      if (!code || !state) {
        return new Response("Bad Request", { status: 400 });
      }
      if (expectedState !== "" && state !== expectedState) {
        return new Response("Forbidden", { status: 403 });
      }
      onHit({ code, state });
      return new Response(
        "<html><body>Sign-in complete. You can close this tab.</body></html>",
        { headers: { "Content-Type": "text/html; charset=utf-8" } },
      );
    },
  });
}

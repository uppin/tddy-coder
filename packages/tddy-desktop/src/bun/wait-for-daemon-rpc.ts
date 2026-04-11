/**
 * Embedded `tddy-daemon` starts asynchronously; Vite and the OAuth relay both need the HTTP
 * server on `listen.web_port` before proxying or `GenerateToken`.
 */

/** `http://host:port/rpc` → `http://host:port` */
export function rpcBaseToHttpOrigin(rpcBaseUrl: string): string | null {
  try {
    return new URL(rpcBaseUrl).origin;
  } catch {
    return null;
  }
}

/**
 * Poll until `GET {origin}/api/config` succeeds (or returns an auth-related status), or timeout.
 */
export async function waitForDaemonHttp(
  rpcBaseUrl: string,
  options: { timeoutMs?: number; pollMs?: number } = {}
): Promise<boolean> {
  const { timeoutMs = 90_000, pollMs = 250 } = options;
  const origin = rpcBaseToHttpOrigin(rpcBaseUrl);
  if (!origin) {
    console.error("[tddy-desktop] Invalid TDDY_RPC_BASE:", rpcBaseUrl);
    return false;
  }
  const deadline = Date.now() + timeoutMs;
  let attempt = 0;
  while (Date.now() < deadline) {
    attempt++;
    try {
      const res = await fetch(`${origin}/api/config`, {
        signal: AbortSignal.timeout(2000),
      });
      if (res.ok || res.status === 401 || res.status === 403) {
        if (attempt > 1) {
          console.info(
            `[tddy-desktop] Daemon HTTP ready at ${origin} (after ${attempt} attempts)`
          );
        }
        return true;
      }
    } catch {
      /* ECONNREFUSED while daemon binds */
    }
    if (attempt === 1 || attempt % 40 === 0) {
      console.info(
        `[tddy-desktop] Waiting for embedded daemon at ${origin}/api/config (OAuth relay and Vite /api proxy need this)…`
      );
    }
    await new Promise((r) => setTimeout(r, pollMs));
  }
  console.error(
    `[tddy-desktop] Timed out waiting for daemon at ${origin}. Start tddy-daemon on that port, or fix embedded spawn (TDDY_DAEMON_BINARY / build).`
  );
  return false;
}

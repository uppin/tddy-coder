/**
 * Structured markers for TDD tracing (grep logs for `"tddy":`). **Development only** — no-ops in
 * production so error aggregators are not flooded.
 */
export function emitTddyMarker(
  marker_id: string,
  scope: string,
  data: Record<string, unknown> = {},
): void {
  if (!import.meta.env.DEV) return;
  console.debug(
    JSON.stringify({
      tddy: { marker_id, scope, data },
    }),
  );
}

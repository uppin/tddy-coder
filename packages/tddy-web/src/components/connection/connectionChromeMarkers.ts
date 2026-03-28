/** Structured stderr-style marker for tracing; also logs at info for dev visibility. */
export function emitConnectionChromeMarker(
  markerId: string,
  scope: string,
  data: Record<string, unknown> = {},
): void {
  const payload = { tddy: { marker_id: markerId, scope, data } };
  console.info("[connection-chrome]", markerId, scope, data);
  console.error(JSON.stringify(payload));
}

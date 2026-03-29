/**
 * Dev/test trace: one-line JSON on stderr so Cypress or shell captures can grep `tddy`.
 * Not gated on environment — same behavior as production builds (per project rules).
 */
export function emitTddyMarker(
  markerId: string,
  scope: string,
  data: Record<string, unknown> = {},
): void {
  const payload = { tddy: { marker_id: markerId, scope, data } };
  console.debug("[tddy][marker]", markerId, scope, data);
  console.info("[tddy][marker]", markerId, scope);
  console.error(JSON.stringify(payload));
}

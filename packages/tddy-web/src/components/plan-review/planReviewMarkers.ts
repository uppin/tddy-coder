/**
 * Trace markers for plan-review entry points (grep-friendly JSON on debug channel).
 */
export function logPlanReviewMarker(
  markerId: string,
  scope: string,
  data: Record<string, unknown> = {},
): void {
  const line = JSON.stringify({
    tddy: { marker_id: markerId, scope, data },
  });
  if (import.meta.env.DEV) {
    console.debug(`[plan-review:marker] ${line}`);
  }
}

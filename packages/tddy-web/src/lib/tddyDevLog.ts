/**
 * Development-only tracing for `[tddy]` diagnostics. No-ops in production builds.
 */
export function tddyDevDebug(scope: string, ...args: unknown[]): void {
  if (!import.meta.env.DEV) return;
  console.debug(`[tddy]${scope}`, ...args);
}

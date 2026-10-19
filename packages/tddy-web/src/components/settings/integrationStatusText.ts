/**
 * One line saying what a daemon integration is doing right now, from `GetIntegrationStatus`.
 *
 * "Off" · "Missing: url, api_secret" · "Connecting…" · "Connected · 3m" ·
 * "Unreachable · 12s: <error>" — the "since" part is how long the integration has been in that
 * state, so an operator who just saved a URL can tell a fresh failure from an old one.
 */

import { IntegrationState, type IntegrationStatus } from "../../gen/daemon_config_pb";

/** Whole seconds under a minute, whole minutes under an hour, whole hours after that. */
function sinceText(sinceUnixMs: bigint, now: Date): string {
  // A daemon whose clock runs ahead of the browser's would otherwise show a negative duration.
  const seconds = Math.max(0, Math.floor((now.getTime() - Number(sinceUnixMs)) / 1000));
  if (seconds < 60) return `${seconds}s`;
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m`;
  return `${Math.floor(seconds / 3600)}h`;
}

export function integrationStatusText(status: IntegrationStatus, now: Date): string {
  switch (status.state) {
    case IntegrationState.DISABLED:
      return "Off";
    case IntegrationState.MISCONFIGURED:
      return `Missing: ${status.missingFields.join(", ")}`;
    case IntegrationState.CONNECTING:
      return "Connecting…";
    case IntegrationState.CONNECTED:
      return `Connected · ${sinceText(status.sinceUnixMs, now)}`;
    case IntegrationState.UNREACHABLE:
      return `Unreachable · ${sinceText(status.sinceUnixMs, now)}: ${status.error}`;
    default:
      // The state enum comes from the same proto as the daemon, so anything else is a contract
      // violation to surface, not a state to paper over with a guessed label.
      throw new Error(`integrationStatusText: unexpected integration state ${status.state}`);
  }
}

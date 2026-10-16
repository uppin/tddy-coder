/**
 * One line saying what a daemon integration is doing right now, from `GetIntegrationStatus`.
 *
 * "Off" · "Missing: url, api_secret" · "Connecting…" · "Connected · 3m" ·
 * "Unreachable · 12s: <error>" — the "since" part is how long the integration has been in that
 * state, so an operator who just saved a URL can tell a fresh failure from an old one.
 */

import type { IntegrationStatus } from "../../gen/daemon_config_pb";

export function integrationStatusText(status: IntegrationStatus, now: Date): string {
  void status;
  void now;
  // TODO(status-ui): implement
  throw new Error("integrationStatusText: not implemented");
}

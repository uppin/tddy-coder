/**
 * Test-only wrapper naming the daemon a mounted terminal surface talks to.
 *
 * The terminal package reads its daemon through the clients port, whose test double
 * (`aClientsPortDouble` in `tddy-client-web/testkit`) already selects the first listed
 * daemon — so mounting through `mountWithRpc` / `cy.mountWithClients` is what seeds the
 * selection. This wrapper keeps the specs' stated fixture ("one local daemon selected")
 * without reaching for the host application's providers.
 */

import React from "react";
import type { DaemonHost } from "tddy-client-web";

/** Default single-daemon fixture — matches the "one local daemon" shape used across the specs. */
export const DEFAULT_TEST_DAEMON: DaemonHost = { instanceId: "local", label: "local (this daemon)" };

/** Wrap `children` as a surface with `daemons` listed and the first one selected (by the port double). */
export function withSelectedDaemon(
  children: React.ReactNode,
  daemons: DaemonHost[] = [DEFAULT_TEST_DAEMON],
): React.ReactElement {
  void daemons;
  return <>{children}</>;
}

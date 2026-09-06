/**
 * Unit tests for the decision a Hosts row makes before it subscribes: which host, if any, this
 * cell's telemetry feed should read.
 *
 * This is a pure function rather than logic inline in the component because it is the one property
 * a component test cannot see. `mountWithRpc` hands every host the same in-memory transport and
 * `StreamHostStatsRequest` carries only a session token, so a cell reading the **selected daemon**
 * for every row renders the same DOM and opens the same number of streams as one reading each row's
 * own host. Both look correct through the fake; only the decision tells them apart.
 *
 * The distinction between `null` and `undefined` is the whole point and is load-bearing:
 * `useHostStats(undefined)` follows the daemon selector, `useHostStats(null)` subscribes to nothing.
 * Returning `undefined` from here would put one machine's CPU under another machine's name.
 *
 * PRD: `docs/ft/web/hosts-screen-telemetry.md` (AC-3, AC-4, AC-5)
 */

import { describe, it, expect } from "bun:test";
import { telemetryFeedFor } from "./hostTelemetryState";

const THIS_HOST = "workstation-1";

/** A host in the live roster that a registered wire can reach. */
function anOnlineRoutableHost() {
  return { instanceId: THIS_HOST, online: true, routable: true };
}

describe("which feed a host row reads", () => {
  it("reads the row's own host when it is online and routable", () => {
    // Given an online host something can reach
    const row = anOnlineRoutableHost();

    // When the row decides what to subscribe to
    const feed = telemetryFeedFor(row);

    // Then it names that host, and no other
    expect(feed).toBe(THIS_HOST);
  });

  it("reads nothing for an offline host", () => {
    // Given a host that has left the roster
    const row = { ...anOnlineRoutableHost(), online: false };

    // When the row decides what to subscribe to
    const feed = telemetryFeedFor(row);

    // Then it subscribes to nothing
    expect(feed).toBeNull();
  });

  it("reads nothing for an online host no wire routes to", () => {
    // Given a host in the roster that no registered provider can reach
    const row = { ...anOnlineRoutableHost(), routable: false };

    // When the row decides what to subscribe to
    const feed = telemetryFeedFor(row);

    // Then it subscribes to nothing rather than guessing
    expect(feed).toBeNull();
  });
});

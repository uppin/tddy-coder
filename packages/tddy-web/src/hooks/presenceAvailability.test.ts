/**
 * Unit tests for the rule every presence-derived surface renders from.
 *
 * The order of the two facts is the behaviour under test. A connection with no `presence` and a
 * join that is still in flight look identical to the capability predicate — neither has a roster
 * *yet* — and only one of them should be told "not available on this connection".
 *
 * Changeset: `docs/dev/1-WIP/2026-09-05-optional-livekit-capability-gating.md`
 */

import { describe, it, expect } from "bun:test";
import { presenceAvailability } from "./presenceAvailability";

const CARRIES_PRESENCE = true;
const CARRIES_NO_PRESENCE = false;

describe("presence availability", () => {
  it("offers the roster on a joined common room over a connection that carries presence", () => {
    // Given a joined common room reached over LiveKit
    // Then the roster is rendered exactly as it always was
    expect(presenceAvailability("connected", CARRIES_PRESENCE)).toBe("available");
  });

  it("names the connection as the reason when nothing is joining and presence is absent", () => {
    // Given a host reached over a frame pipe: no common room is being joined, and none ever will be
    // Then the surface says the roster is not available on this connection, rather than sitting on
    // the "Connecting…" placeholder it used to show forever
    expect(presenceAvailability("idle", CARRIES_NO_PRESENCE)).toBe("unavailable");
  });

  it("keeps saying the join is in flight while it is, rather than blaming the connection", () => {
    // Given a common room mid-join — `LiveKitConnections` is still bound to a null room, so the
    // capability is absent for a second or two on a page that will have it
    // Then the surface waits instead of announcing a permanent absence it would then contradict
    expect(presenceAvailability("connecting", CARRIES_NO_PRESENCE)).toBe("connecting");
  });

  it("quotes a failed join rather than reporting it as a connection without presence", () => {
    // Given a join that failed (blocked ICE, an unreachable LiveKit URL)
    // Then the reason LiveKit gave is what the operator needs, not a capability verdict
    expect(presenceAvailability("error", CARRIES_NO_PRESENCE)).toBe("error");
  });
});

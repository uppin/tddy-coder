/**
 * Unit tests for the rule every capability-gated surface renders from.
 *
 * The order of the two facts is the behaviour under test. A connection that does not carry a
 * capability and a join that is still in flight look identical to the capability predicate —
 * neither has it *yet* — and only one of them should be told "not available on this connection".
 *
 * Changeset: `docs/dev/1-WIP/2026-09-05-optional-livekit-capability-gating.md`
 */

import { describe, it, expect } from "bun:test";
import { capabilityAvailability } from "./capabilityAvailability";

const CARRIES_IT = true;
const DOES_NOT_CARRY_IT = false;

describe("capability availability", () => {
  it("offers the surface on a joined common room over a connection that carries the capability", () => {
    // Given a joined common room reached over LiveKit
    // Then the roster, the VNC tab and everything else on that wire render exactly as they always did
    expect(capabilityAvailability("connected", CARRIES_IT)).toBe("available");
  });

  it("names the connection as the reason when nothing is joining and the capability is absent", () => {
    // Given a host reached over a frame pipe: no common room is being joined, and none ever will be
    // Then the surface says it is not available on this connection, rather than sitting on the
    // "Connecting…" placeholder it used to show forever
    expect(capabilityAvailability("idle", DOES_NOT_CARRY_IT)).toBe("unavailable");
  });

  it("keeps saying the join is in flight while it is, rather than blaming the connection", () => {
    // Given a common room mid-join — `LiveKitConnections` is still bound to a null room, so every
    // capability is absent for a second or two on a page that will have them all
    // Then the surface waits instead of announcing a permanent absence it would then contradict
    expect(capabilityAvailability("connecting", DOES_NOT_CARRY_IT)).toBe("connecting");
  });

  it("quotes a failed join rather than reporting it as a connection without the capability", () => {
    // Given a join that failed (blocked ICE, an unreachable LiveKit URL)
    // Then the reason LiveKit gave is what the operator needs, not a capability verdict
    expect(capabilityAvailability("error", DOES_NOT_CARRY_IT)).toBe("error");
  });
});

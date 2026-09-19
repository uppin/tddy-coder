/**
 * Unit tests for the create-session placement algebra.
 *
 * Three controls — Sandbox, Managed codebase and Sandboxed codebase — each name a placement, and a
 * session has exactly one. The rule lives in its own module rather than inline in
 * `CreateSessionPane.tsx` (1412 lines, and on record as needing a split of its own:
 * docs/dev/todo/2026-08-19-session-creation-agent-catalog-the-rest-of-the-fan-out.md), so the
 * algebra can be tested exhaustively and the pane grows only by the control and its wiring.
 *
 * Run as a Cypress spec because that is this package's only working test runner — `tsc` is not a
 * gate here. Nothing is mounted: these are pure-function tests.
 *
 * PRD: docs/ft/daemon/amendments/PRD-2026-09-18-sandboxed-codebase-from-the-web.md
 * Changeset: docs/dev/1-WIP/2026-09-18-sandboxed-codebase-mode-from-the-web.md
 */

import {
  placementAfterToggling,
  sandboxedCodebaseUnavailability,
  type CodebasePlacementChoice,
} from "../../src/components/sessions/codebasePlacement";
import type { DaemonHost } from "../../src/lib/participantRole";

const NONE: CodebasePlacementChoice = "none";
const AGENT_SANDBOX: CodebasePlacementChoice = "sandbox";
const MANAGED: CodebasePlacementChoice = "managed";
const JAILED_CODEBASE: CodebasePlacementChoice = "sandboxedCodebase";

describe("codebase placement rules", () => {
  it("chooses the jailed-codebase placement from nothing chosen", () => {
    // Given no placement chosen
    // When the operator jails the codebase
    const placement = placementAfterToggling(NONE, JAILED_CODEBASE, true);

    // Then that is the placement
    expect(placement).to.equal(JAILED_CODEBASE);
  });

  it("replaces the managed placement when the codebase is jailed", () => {
    // Given the managed placement, which jails the agent and leaves the code on the host
    // When the operator jails the codebase instead
    const placement = placementAfterToggling(MANAGED, JAILED_CODEBASE, true);

    // Then the managed placement is gone — a session has one placement, not two
    expect(placement).to.equal(JAILED_CODEBASE);
  });

  it("replaces the agent sandbox when the codebase is jailed", () => {
    // Given the agent jailed
    // When the operator jails the codebase instead
    const placement = placementAfterToggling(AGENT_SANDBOX, JAILED_CODEBASE, true);

    // Then the opposite placement replaces it outright
    expect(placement).to.equal(JAILED_CODEBASE);
  });

  it("replaces the jailed codebase when the managed placement is chosen", () => {
    // Given the codebase jailed
    // When the operator chooses the managed placement
    const placement = placementAfterToggling(JAILED_CODEBASE, MANAGED, true);

    // Then — the rule holds in both directions, or the form would let two placements stand
    expect(placement).to.equal(MANAGED);
  });

  it("replaces the jailed codebase when the agent sandbox is chosen", () => {
    // Given the codebase jailed
    // When the operator chooses to jail the agent
    const placement = placementAfterToggling(JAILED_CODEBASE, AGENT_SANDBOX, true);

    // Then
    expect(placement).to.equal(AGENT_SANDBOX);
  });

  it("leaves no placement when the only chosen one is unchecked", () => {
    // Given the codebase jailed
    // When the operator unchecks it
    const placement = placementAfterToggling(JAILED_CODEBASE, JAILED_CODEBASE, false);

    // Then the session has no placement — the default every session had before any of this
    expect(placement).to.equal(NONE);
  });

  it("leaves the chosen placement alone when a different one is unchecked", () => {
    // Given the codebase jailed
    // When the operator unchecks the managed toggle, which was not on
    const placement = placementAfterToggling(JAILED_CODEBASE, MANAGED, false);

    // Then nothing moves. Unchecking an already-clear control must not clear the chosen one.
    expect(placement).to.equal(JAILED_CODEBASE);
  });
});

describe("sandboxed codebase availability", () => {
  const aHostServingIt: DaemonHost = {
    instanceId: "workstation",
    label: "workstation (this daemon)",
    sandboxedCodebase: { confinesFilesystem: true },
  };

  const aHostWhoseJailSharesTheFilesystemRoot: DaemonHost = {
    instanceId: "buildbox",
    label: "buildbox",
    sandboxedCodebase: { confinesFilesystem: false },
  };

  const aHostThatDoesNotAdvertiseIt: DaemonHost = {
    instanceId: "old-daemon",
    label: "old-daemon",
  };

  it("serves the placement on a host that advertises it", () => {
    // Given a host advertising the capability
    // When
    const reason = sandboxedCodebaseUnavailability(aHostServingIt);

    // Then there is no reason to withhold it
    expect(reason).to.equal(null);
  });

  it("serves the placement on a host whose jail shares the filesystem root", () => {
    // Given a host that serves it with a weaker jail
    // When
    const reason = sandboxedCodebaseUnavailability(aHostWhoseJailSharesTheFilesystemRoot);

    // Then it is still offered — what that jail does not confine is a caveat beside an enabled
    // control, not a reason to take the placement away
    expect(reason).to.equal(null);
  });

  it("names the host when it does not advertise the placement at all", () => {
    // Given a daemon old enough not to advertise the capability
    // When
    const reason = sandboxedCodebaseUnavailability(aHostThatDoesNotAdvertiseIt);

    // Then the control is withheld with a reason naming the host, so the operator learns which
    // machine to look at rather than finding the option silently missing
    expect(reason).to.be.a("string");
    expect(reason).to.contain("old-daemon");
  });
});

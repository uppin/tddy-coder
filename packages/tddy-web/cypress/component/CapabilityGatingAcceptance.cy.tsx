/**
 * Acceptance spec: a media or presence surface is absent when the connection cannot serve it.
 *
 * On a host reached without LiveKit these surfaces render unconditionally today, which means a VNC
 * tab that never paints, a screen-sharing overlay with nothing to subscribe to, a participant list
 * that is permanently empty, and a sessions drawer that silently loses every cross-host row —
 * because that reconciliation is itself built on participants.
 *
 * Two decisions are pinned here. **Hidden, not disabled**: a disabled VNC tab invites a support
 * question with no good answer, while an absent one matches the truth — this host is reached a way
 * that has no video. And **one predicate**: nothing re-derives capability from a `Room`.
 *
 * Changeset: `docs/dev/1-WIP/2026-09-05-optional-livekit-capability-gating.md`
 * Stack: `optional-livekit` node 4 of 7.
 */

import React from "react";
import { useHasCapability } from "../../src/rpc/connections/useHasCapability";
import type { ConnectionCapability } from "../../src/rpc/connections/types";
import { byTestId } from "../support/testIds";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

function aConnectionThatCan(...capabilities: ConnectionCapability[]) {
  return { capabilities: new Set(capabilities) };
}

/** A connection carried over LiveKit. */
const FULLY_CAPABLE = aConnectionThatCan("rpc", "media", "presence");

/** A connection over a frame pipe: the desktop app reaching its own host. */
const RPC_ONLY = aConnectionThatCan("rpc");

// ---------------------------------------------------------------------------
// Probes
// ---------------------------------------------------------------------------

/**
 * A stand-in for the session tab strip, gating the two media tabs and rendering only what applies.
 *
 * The real tabs are `SessionVncTab` and `SessionScreenSharingTab`; this asserts the *gating rule*
 * they will be wired to, which is what this node owns. Wiring each real surface is `/green`.
 */
function SessionTabsProbe({ connection }: { connection: { capabilities: Set<ConnectionCapability> } }) {
  const media = useHasCapability(connection, "media");
  const presence = useHasCapability(connection, "presence");
  return (
    <div>
      <div data-testid="tabs">
        {["terminal", ...(media ? ["vnc", "screen-sharing"] : []), ...(presence ? ["participants"] : [])].join(",")}
      </div>
      {media && <div data-testid="vnc-tab">VNC</div>}
      {media && <div data-testid="screen-sharing-tab">Screen sharing</div>}
      {presence && <div data-testid="participants-panel">Participants</div>}
    </div>
  );
}

/**
 * A stand-in for the sessions drawer's cross-host reconciliation, which is presence-derived.
 *
 * Without presence it must show what `ListSessions` reports and stop implying the list is complete —
 * degrading honestly rather than reading as "this host has no sessions".
 */
function CrossHostRowsProbe({ connection }: { connection: { capabilities: Set<ConnectionCapability> } }) {
  const presence = useHasCapability(connection, "presence");
  return (
    <div>
      <div data-testid="rows">{presence ? "local + cross-host" : "local only"}</div>
      <div data-testid="completeness">
        {presence ? "all hosts" : "sessions on other hosts are not visible from this connection"}
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Specs
// ---------------------------------------------------------------------------

describe("a connection that carries tracks and a roster", () => {
  it("offers every surface, exactly as today", () => {
    cy.mount(<SessionTabsProbe connection={FULLY_CAPABLE} />);

    // Then nothing about the LiveKit experience changes — the whole stack turns on this staying true
    byTestId("tabs").should("have.text", "terminal,vnc,screen-sharing,participants");
    byTestId("vnc-tab").should("exist");
    byTestId("screen-sharing-tab").should("exist");
    byTestId("participants-panel").should("exist");
  });
});

describe("a connection that is plain RPC", () => {
  it("removes the media tabs from navigation rather than disabling them", () => {
    cy.mount(<SessionTabsProbe connection={RPC_ONLY} />);

    // Then the VNC and screen-sharing tabs are gone from the strip, not present-and-dead. A tab the
    // user cannot use is worse than a tab that is not there. The strip itself is asserted first, so
    // a component that throws cannot satisfy the `not.exist` checks below.
    byTestId("tabs").should("have.text", "terminal");
    byTestId("vnc-tab").should("not.exist");
    byTestId("screen-sharing-tab").should("not.exist");
  });

  it("removes the participant panel", () => {
    cy.mount(<SessionTabsProbe connection={RPC_ONLY} />);

    // The positive assertion comes first on purpose: `should("not.exist")` alone is satisfied by
    // the component failing to render at all, so on its own it would pass against a predicate that
    // throws. Pinning the strip that *does* render makes the absence mean absence.
    byTestId("tabs").should("have.text", "terminal");
    byTestId("participants-panel").should("not.exist");
  });

  it("degrades the cross-host session list honestly instead of claiming it is complete", () => {
    cy.mount(<CrossHostRowsProbe connection={RPC_ONLY} />);

    // Then the drawer shows what ListSessions reports and says why the rest is missing. Silently
    // dropping the cross-host rows is what would read as "this host has no sessions".
    byTestId("rows").should("have.text", "local only");
    byTestId("completeness").should(
      "have.text",
      "sessions on other hosts are not visible from this connection",
    );
  });

  it("still keeps the cross-host list complete when presence is available", () => {
    cy.mount(<CrossHostRowsProbe connection={FULLY_CAPABLE} />);

    byTestId("rows").should("have.text", "local + cross-host");
    byTestId("completeness").should("have.text", "all hosts");
  });
});

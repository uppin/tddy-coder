/**
 * Unit tests for the capability predicate every media and presence surface is gated on.
 *
 * The point of a single predicate is that no surface re-derives "can I show video here" from the
 * presence of a `Room`, from a transport, or from a status string. Three nodes have now added
 * capability information; a fourth reading of it would be the drift that undoes the stack.
 *
 * Changeset: `docs/dev/1-WIP/2026-09-05-optional-livekit-capability-gating.md`
 */

import { describe, it, expect } from "bun:test";
import { hasCapability } from "./useHasCapability";
import type { ConnectionCapability } from "./types";

function aConnectionThatCan(...capabilities: ConnectionCapability[]) {
  return { capabilities: new Set(capabilities) };
}

/** What a LiveKit-carried connection advertises. */
function aFullyCapableConnection() {
  return aConnectionThatCan("rpc", "media", "presence");
}

/** What a connection over a frame pipe advertises — no tracks, no roster. */
function anRpcOnlyConnection() {
  return aConnectionThatCan("rpc");
}

describe("capability gating", () => {
  it("lets a LiveKit-carried connection show video surfaces", () => {
    // Given a fully capable connection
    const connection = aFullyCapableConnection();

    // Then VNC, screen sharing and participant video all apply
    expect(hasCapability(connection, "media")).toBe(true);
    expect(hasCapability(connection, "presence")).toBe(true);
    expect(hasCapability(connection, "rpc")).toBe(true);
  });

  it("refuses media on a connection that carries no tracks", () => {
    // Given a connection over a frame pipe — the desktop app over IPC
    const connection = anRpcOnlyConnection();

    // Then the VNC tab, the screen-sharing overlay and the video preview do not apply. They are
    // absent rather than broken: a frame pipe cannot carry a track, so there is nothing to degrade.
    expect(hasCapability(connection, "media")).toBe(false);
  });

  it("refuses presence on a connection with no participant roster", () => {
    // Given the same connection
    const connection = anRpcOnlyConnection();

    // Then the participant list, the rooms panel and the cross-host session reconciliation that is
    // built on participants all know they have nothing to read
    expect(hasCapability(connection, "presence")).toBe(false);
  });

  it("still allows rpc, which every connection has", () => {
    // Given the least capable connection there is
    const connection = anRpcOnlyConnection();

    // Then it can still make calls — which is why `rpc` is in the vocabulary at all: a capability
    // set is never empty, and a caller can ask the same question of every capability.
    expect(hasCapability(connection, "rpc")).toBe(true);
  });

  it("answers false for a connection that does not exist", () => {
    // Given no host selected, or a host nothing can reach
    // Then every surface is hidden, without the caller special-casing null. A caller that needs to
    // tell "no host" from "host without video" is asking a routing question and should read status.
    expect(hasCapability(null, "media")).toBe(false);
    expect(hasCapability(undefined, "presence")).toBe(false);
    expect(hasCapability(null, "rpc")).toBe(false);
  });

  it("answers false for a connection advertising nothing at all", () => {
    // Given a connection still negotiating what it can do
    const connection = aConnectionThatCan();

    // Then nothing is offered until it says otherwise — the safe direction, since showing a surface
    // that cannot work is worse than showing one late
    expect(hasCapability(connection, "media")).toBe(false);
    expect(hasCapability(connection, "rpc")).toBe(false);
  });
});

/**
 * Unit tests for the layout's host gate: when a host-scoped screen has no viable RPC client for the
 * selected host, the screen is blocked and says why, instead of rendering controls that cannot work.
 *
 * "Viable" is read the way the connection layer offers it: a connection exists for the selected
 * host, and — when that connection rides the common room — the room has not failed. A connection
 * that exists but is still settling stays open: single calls on it report their own failures inline.
 */

import { describe, it, expect } from "bun:test";
import { hostGateOf, type HostGate, type HostGateInput } from "./hostGate";

function anInput(overrides: Partial<HostGateInput> = {}): HostGateInput {
  return {
    selectedInstanceId: "workstation-1",
    hostLabel: "workstation-1",
    connection: { providerId: "livekit" },
    liveKit: { status: "connected", error: null },
    ...overrides,
  };
}

/** Why a blocked gate blocks; `null` for an open gate or a blocked one that gives no reason. */
function blockedReasonOf(gate: HostGate): string | null {
  return gate.kind === "blocked" ? gate.reason : null;
}

describe("hostGateOf", () => {
  it("is open when a connection reaches the selected host over a joined common room", () => {
    // Given a LiveKit connection over a joined room
    const input = anInput();

    // When / Then
    expect(hostGateOf(input)).toEqual({ kind: "open" });
  });

  it("is open while a connection exists but the room is still settling", () => {
    // Given
    const input = anInput({ liveKit: { status: "connecting", error: null } });

    // When / Then
    expect(hostGateOf(input)).toEqual({ kind: "open" });
  });

  it("is open for a host reached over IPC, whatever the common room is doing", () => {
    // Given
    const input = anInput({
      connection: { providerId: "ipc" },
      liveKit: { status: "error", error: "validate request timed out" },
    });

    // When / Then
    expect(hostGateOf(input)).toEqual({ kind: "open" });
  });

  it("blocks when no host is selected", () => {
    // Given
    const input = anInput({ selectedInstanceId: null, hostLabel: null, connection: null });

    // When
    const gate = hostGateOf(input);

    // Then
    expect(gate).toMatchObject({ kind: "blocked", tone: "error", title: "No host selected" });
  });

  it("blocks with the common room's failure when the join failed and nothing reaches the host", () => {
    // Given the LiveKit-server-down case: the join failed, so no room and no connection
    const input = anInput({
      connection: null,
      liveKit: { status: "error", error: "could not establish signal connection" },
    });

    // When
    const gate = hostGateOf(input);

    // Then
    expect(gate).toMatchObject({
      kind: "blocked",
      tone: "error",
      title: "Not connected to workstation-1",
    });
    expect(blockedReasonOf(gate)).toContain("could not establish signal connection");
  });

  it("blocks with the room's failure when a joined room dropped under a LiveKit connection", () => {
    // Given
    const input = anInput({ liveKit: { status: "error", error: "LiveKit disconnected (reason=SIGNAL_CLOSE)." } });

    // When
    const gate = hostGateOf(input);

    // Then
    expect(gate).toMatchObject({ kind: "blocked", tone: "error" });
    expect(blockedReasonOf(gate)).toContain("LiveKit disconnected (reason=SIGNAL_CLOSE).");
  });

  it("says it is connecting, not failing, while the common room is still being joined", () => {
    // Given
    const input = anInput({ connection: null, liveKit: { status: "connecting", error: null } });

    // When
    const gate = hostGateOf(input);

    // Then
    expect(gate).toMatchObject({
      kind: "blocked",
      tone: "connecting",
      title: "Connecting to workstation-1…",
    });
  });

  it("says the host is not in the room when the room is up but does not reach it", () => {
    // Given
    const input = anInput({ connection: null, liveKit: { status: "connected", error: null } });

    // When
    const gate = hostGateOf(input);

    // Then
    expect(gate).toMatchObject({ kind: "blocked", tone: "error" });
    expect(blockedReasonOf(gate)).toContain("not in the common room");
  });

  it("says no connection is configured when there is no common room and no other wire", () => {
    // Given
    const input = anInput({ connection: null, liveKit: { status: "idle", error: null } });

    // When
    const gate = hostGateOf(input);

    // Then
    expect(gate).toMatchObject({ kind: "blocked", tone: "error" });
    expect(blockedReasonOf(gate)).toContain("LiveKit is not configured");
  });

  it("falls back to the instance id when the host has no label", () => {
    // Given
    const input = anInput({ hostLabel: null, connection: null, liveKit: undefined });

    // When
    const gate = hostGateOf(input);

    // Then
    expect(gate).toMatchObject({ kind: "blocked", title: "Not connected to workstation-1" });
  });
});

/**
 * Unit tests for the session-participant RPC client builder — `ConnectionService` clients for an
 * attached session target the session participant identity (`daemon-<instanceId>-<sessionId>`),
 * not the daemon participant (`daemon-<instanceId>`).
 *
 * Changeset: `2026-07-12-fast-session-change`
 * Feature: `docs/ft/web/session-drawer.md#fast-session-change` (req 1)
 *
 * ⚠️ RED PHASE — fails until `./sessionParticipantRpcClient` exists with the API below.
 */

import { describe, it, expect } from "bun:test";
import type { Transport } from "@connectrpc/connect";
import {
  buildSessionParticipantRpcClient,
  sessionParticipantIdentity,
} from "./sessionParticipantRpcClient";

/** Fake LiveKit factory that records the target identity and returns a stub transport. */
function aRecordingFactory(targets: string[]): (room: unknown, targetIdentity: string) => Transport {
  return (_room, targetIdentity) => {
    targets.push(targetIdentity);
    return { kind: "transport" } as unknown as Transport;
  };
}

describe("sessionParticipantIdentity", () => {
  it("derives daemon-<instanceId>-<sessionId> for a session on a multi-host daemon", () => {
    expect(sessionParticipantIdentity("workstation-1", "aaaaaaaa-0000-4000-8000-000000000001"))
      .toBe("daemon-workstation-1-aaaaaaaa-0000-4000-8000-000000000001");
  });
});

describe("buildSessionParticipantRpcClient", () => {
  it("builds a ConnectionService client targeting the session participant identity", () => {
    // Given
    const targets: string[] = [];
    const factory = aRecordingFactory(targets);
    const sessionId = "aaaaaaaa-0000-4000-8000-000000000001";
    const daemonInstanceId = "workstation-1";

    // When
    const client = buildSessionParticipantRpcClient(
      factory,
      {} as unknown,
      sessionId,
      daemonInstanceId,
    );

    // Then — a ConnectionService client is returned and the factory was called with the
    // session participant identity (not the daemon participant identity).
    expect(typeof client.executeTool).toBe("function");
    expect(targets).toEqual([sessionParticipantIdentity(daemonInstanceId, sessionId)]);
    expect(targets).not.toContain(`daemon-${daemonInstanceId}`);
  });
});

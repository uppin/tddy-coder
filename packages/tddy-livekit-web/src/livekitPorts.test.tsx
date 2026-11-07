import { afterEach, describe, expect, test } from "bun:test";
import { cleanup, renderHook } from "@testing-library/react";
import type { ReactNode } from "react";
import { useParticipantPresent, useParticipantProbe } from "tddy-client-web";
import { LiveKitPortsProvider, livekitPorts } from "./livekitPorts";
import { aJoinedRoom } from "./testkit";
import { asRoom } from "./roomHandle";

afterEach(cleanup);

function inLiveKitPorts({ children }: { children: ReactNode }) {
  return <LiveKitPortsProvider ports={livekitPorts()}>{children}</LiveKitPortsProvider>;
}

describe("the LiveKit room-RPC port", () => {
  test("a send-time probe sees a presenter that left after the last render", () => {
    // Given
    const room = aJoinedRoom(["daemon-host-a"]);
    const { result } = renderHook(
      () => ({ rendered: useParticipantPresent(room, "daemon-host-a"), probe: useParticipantProbe(room, "daemon-host-a") }),
      { wrapper: inLiveKitPorts },
    );

    // When
    asRoom(room)!.remoteParticipants.delete("daemon-host-a");

    // Then
    expect(result.current.rendered).toBe(true);
    expect(result.current.probe()).toBe(false);
  });

  test("a send-time probe sees a presenter that joined after the last render", () => {
    // Given
    const room = aJoinedRoom([]);
    const { result } = renderHook(() => useParticipantProbe(room, "daemon-host-a"), { wrapper: inLiveKitPorts });

    // When
    (asRoom(room)!.remoteParticipants as Map<string, unknown>).set("daemon-host-a", { identity: "daemon-host-a" });

    // Then
    expect(result.current()).toBe(true);
  });

  test("without a room the probe answers no", () => {
    // Given / When
    const { result } = renderHook(() => useParticipantProbe(null, "daemon-host-a"), { wrapper: inLiveKitPorts });

    // Then
    expect(result.current()).toBe(false);
  });
});

import { afterEach, describe, expect, test } from "bun:test";
import { act, cleanup, renderHook } from "@testing-library/react";
import type { ReactNode } from "react";
import { aPresenceDouble } from "../testkit";
import { PresencePortProvider, type PresencePort, useHostPresence, useMediaPort, useRoomParticipants } from "./media";

afterEach(cleanup);

function inPresence(port: PresencePort) {
  return ({ children }: { children: ReactNode }) => <PresencePortProvider port={port}>{children}</PresencePortProvider>;
}

const ALICE = { identity: "browser-alice", role: "browser", joinedAt: 1, metadata: "", codexOAuth: null };
const DAEMON = { identity: "daemon-host-a", role: "daemon", joinedAt: 2, metadata: "", codexOAuth: null };

describe("the presence port", () => {
  test("useHostPresence returns the room the port reports for a host", () => {
    // Given
    const presence = aPresenceDouble();
    presence.presentHost("host-a", "common");

    // When
    const { result } = renderHook(() => useHostPresence("host-a"), { wrapper: inPresence(presence) });

    // Then
    expect(result.current).toBe(presence.room("common"));
  });

  test("useRoomParticipants lists the port's participants and re-renders on change", () => {
    // Given
    const presence = aPresenceDouble();
    const room = presence.room("common");
    presence.setParticipants(room, [ALICE]);
    const { result } = renderHook(() => useRoomParticipants(room), { wrapper: inPresence(presence) });

    // When
    act(() => presence.setParticipants(room, [ALICE, DAEMON]));

    // Then
    expect(result.current.map((p) => p.identity)).toEqual(["browser-alice", "daemon-host-a"]);
  });

  test("a host with no presence room reads null", () => {
    // Given
    const presence = aPresenceDouble();

    // When
    const { result } = renderHook(() => useHostPresence("host-b"), { wrapper: inPresence(presence) });

    // Then
    expect(result.current).toBeNull();
  });

  test("useMediaPort outside a provider throws naming the port", () => {
    // When / Then
    expect(() => renderHook(() => useMediaPort())).toThrow(/MediaPortProvider/);
  });
});

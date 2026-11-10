import { afterEach, describe, expect, test } from "bun:test";
import { act, cleanup, renderHook } from "@testing-library/react";
import type { ReactNode } from "react";
import { anInMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { create } from "@bufbuild/protobuf";
import { TokenService, GenerateTokenResponseSchema } from "../gen/token_pb";
import { aPresenceDouble, aRoomRpcDouble } from "../testkit";
import {
  PresencePortProvider,
  type PresencePort,
  RoomRpcPortProvider,
  type RoomRpcPort,
  useHostPresence,
  useMediaPort,
  useParticipantPresent,
  useParticipantProbe,
  useRoomClient,
  useRoomParticipants,
} from "./media";

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

function inRoomRpc(port: RoomRpcPort) {
  return ({ children }: { children: ReactNode }) => <RoomRpcPortProvider port={port}>{children}</RoomRpcPortProvider>;
}

describe("the room-RPC port", () => {
  test("useRoomClient calls the participant through the port", async () => {
    // Given
    const backend = anInMemoryRpcBackend().onUnary(TokenService.method.generateToken, (req) =>
      create(GenerateTokenResponseSchema, { token: `for-${req.identity}`, ttlSeconds: 60n }),
    );
    const port = aRoomRpcDouble(backend);
    const room = aPresenceDouble().room("session-room");

    // When
    const { result } = renderHook(() => useRoomClient(room, "daemon-host-a", TokenService), { wrapper: inRoomRpc(port) });
    const reply = await result.current!.generateToken({ room: "session-room", identity: "alice" });

    // Then
    expect(reply.token).toBe("for-alice");
  });

  test("useRoomClient is null until the room is known", () => {
    // Given
    const port = aRoomRpcDouble(anInMemoryRpcBackend());

    // When
    const { result } = renderHook(() => useRoomClient(null, "daemon-host-a", TokenService), { wrapper: inRoomRpc(port) });

    // Then
    expect(result.current).toBeNull();
  });

  test("useParticipantPresent follows the participant joining and leaving", () => {
    // Given
    const port = aRoomRpcDouble(anInMemoryRpcBackend());
    const room = aPresenceDouble().room("session-room");
    const { result } = renderHook(() => useParticipantPresent(room, "daemon-host-a"), { wrapper: inRoomRpc(port) });

    // When
    act(() => port.setPresent(room, "daemon-host-a", true));

    // Then
    expect(result.current).toBe(true);
  });

  test("useParticipantProbe answers at call time, before any re-render", () => {
    // Given
    const port = aRoomRpcDouble(anInMemoryRpcBackend());
    const room = aPresenceDouble().room("session-room");
    let renders = 0;
    const { result } = renderHook(
      () => {
        renders += 1;
        return useParticipantProbe(room, "daemon-host-a");
      },
      { wrapper: inRoomRpc(port) },
    );
    const probe = result.current;
    const rendersBefore = renders;

    // When
    port.setPresent(room, "daemon-host-a", true);

    // Then
    expect(renders).toBe(rendersBefore);
    expect(probe()).toBe(true);
  });
});

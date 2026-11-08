import { describe, expect, test } from "bun:test";
import { aMediaDouble, aNavigationDouble, aPresenceDouble, aRoomRpcDouble } from "tddy-client-web/testkit";
import { anInMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { portsInScope } from "./portsInScope";
import type { AppPorts } from "./ports";

const aCompleteSetOfPorts = (): AppPorts => ({
  navigation: aNavigationDouble("/"),
  presence: aPresenceDouble(),
  media: aMediaDouble(),
  roomRpc: aRoomRpcDouble(anInMemoryRpcBackend()),
});

describe("the ports the stateful providers read from the app container", () => {
  test("outside a composed app, naming the missing ports and the provider", () => {
    expect(() => portsInScope(null)).toThrow(/navigation.*LiveKit.*AppPortsScope/s);
  });

  test("a handler without room RPC, naming the port", () => {
    const { roomRpc: _omitted, ...withoutRoomRpc } = aCompleteSetOfPorts();
    expect(() => portsInScope(withoutRoomRpc as AppPorts)).toThrow(/roomRpc/);
  });

  test("a complete set is handed through untouched", () => {
    const ports = aCompleteSetOfPorts();
    expect(portsInScope(ports).navigation).toBe(ports.navigation);
  });
});

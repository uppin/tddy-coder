/**
 * Mount a live chat against an in-memory backend through the room-RPC port.
 *
 * A live chat opens its stream through `useRoomClient(room, identity, service)`; the port double
 * answers every client from `backend` once the room is known and the serving participant is present,
 * so the spec hands the chat a real (opaque) room handle instead of `null`.
 */
import type { ReactElement } from "react";
import type { InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { RoomRpcPortProvider, type RoomHandle } from "tddy-client-web";
import { aPresenceDouble, aRoomRpcDouble } from "tddy-client-web/testkit";

/** The identity `AgentChat` addresses when its caller names none. */
export const CHAT_SERVER_IDENTITY = "server";

export function mountWithChatRoom(
  chat: (room: RoomHandle) => ReactElement,
  backend: InMemoryRpcBackend,
  identity: string = CHAT_SERVER_IDENTITY,
): Cypress.Chainable {
  const room = aPresenceDouble().room("chat-room");
  const rooms = aRoomRpcDouble(backend);
  rooms.setPresent(room, identity, true);
  return cy.mountWithClients(<RoomRpcPortProvider port={rooms}>{chat(room)}</RoomRpcPortProvider>, backend);
}

/**
 * Mount a view that opens the session's presenter room (`WorkflowChatScreen`, `PrStackScreen`) against
 * an in-memory backend.
 *
 * The view asks the media port for the presenter room its attachment hint names and talks to the
 * session over the room-RPC port, so the spec supplies both doubles: the hint's room is connected, and
 * the session's serving participant is present in it.
 */
import type { ReactElement } from "react";
import type { InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { MediaPortProvider, RoomRpcPortProvider, type MediaPort, type RoomHandle, type SessionAttachmentHint } from "tddy-client-web";
import { aMediaDouble, aPresenceDouble, aRoomRpcDouble } from "tddy-client-web/testkit";

export const PRESENTER_ROOM = "presenter-room";
export const PRESENTER_IDENTITY = "server";

/** The attachment hint of a session served over {@link PRESENTER_ROOM}. */
export function anAttachmentHint(sessionId: string): SessionAttachmentHint {
  return { sessionId, room: PRESENTER_ROOM, url: "wss://livekit.internal:7880", serverIdentity: PRESENTER_IDENTITY };
}

export function mountWithPresenterRoom(
  view: ReactElement | ((room: RoomHandle) => ReactElement),
  backend: InMemoryRpcBackend,
  mediaOverrides: Partial<MediaPort> = {},
): Cypress.Chainable {
  const room = aPresenceDouble().room(PRESENTER_ROOM);
  const media = aMediaDouble(mediaOverrides);
  media.presentRoom(PRESENTER_ROOM, { room, status: "connected" });
  const rooms = aRoomRpcDouble(backend);
  rooms.setPresent(room, PRESENTER_IDENTITY, true);
  return cy.mountWithClients(
    <MediaPortProvider port={media}>
      <RoomRpcPortProvider port={rooms}>{typeof view === "function" ? view(room) : view}</RoomRpcPortProvider>
    </MediaPortProvider>,
    backend,
  );
}

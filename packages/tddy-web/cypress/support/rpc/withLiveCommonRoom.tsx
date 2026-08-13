/**
 * Mount a subtree under a `SelectedDaemonProvider` that joins the common room for real.
 *
 * `withSelectedDaemon`'s `room`/`daemons` props are *result* seams: they hand the provider an
 * already-joined room and a ready daemon list, bypassing `useCommonRoom` entirely, so no test using
 * them can observe what happens while joining the room — or when joining fails. This helper leaves
 * the provider on its production path (authenticate → mint a LiveKit token over the in-memory
 * backend → `connect()`) and only substitutes the `Room` object itself, which is what lets a test
 * drive a connection failure and assert on what the operator is told.
 */

import React from "react";
import type { Room } from "livekit-client";
import { AuthProvider } from "../../../src/hooks/authProvider";
import { SelectedDaemonProvider } from "../../../src/rpc/selectedDaemon";
import { TokenService } from "../../../src/gen/token_pb";
import { mountWithRpc } from "./inMemory";
import {
  anAuthRefreshBackend,
  CURRENT_ACCESS_TOKEN,
  VALID_REFRESH_TOKEN,
  ACCESS_TOKEN_KEY,
  REFRESH_TOKEN_KEY,
} from "./authRefreshBackend";

/** The `livekit.public_url` a browser is handed by the daemon's `/api/config`. */
export const COMMON_ROOM_LIVEKIT_URL = "ws://livekit.test:7880";
/** The daemon's `livekit.common_room`. */
export const COMMON_ROOM_NAME = "tddy-lobby";
/** The instance id of the daemon that served the bundle (`/api/config`'s `daemon_instance_id`). */
export const SERVING_INSTANCE_ID = "udoo";

/**
 * Mount `children` as a signed-in operator whose browser attempts to join the common room using
 * `room`. Nothing else about the provider is faked: the daemon list is derived from the room's
 * participants exactly as in production, so a room that never connects yields no daemons.
 */
export function mountWithLiveCommonRoom(children: React.ReactNode, room: Room): Cypress.Chainable {
  window.localStorage.setItem(ACCESS_TOKEN_KEY, CURRENT_ACCESS_TOKEN);
  window.localStorage.setItem(REFRESH_TOKEN_KEY, VALID_REFRESH_TOKEN);

  const backend = anAuthRefreshBackend().implement(TokenService, {
    generateToken: async () => ({ token: "common-room-jwt", ttlSeconds: 600n }),
    refreshToken: async () => ({ token: "common-room-jwt", ttlSeconds: 600n }),
  });

  return mountWithRpc(
    <AuthProvider>
      <SelectedDaemonProvider
        livekitUrl={COMMON_ROOM_LIVEKIT_URL}
        commonRoom={COMMON_ROOM_NAME}
        servingInstanceId={SERVING_INSTANCE_ID}
        roomFactory={() => room}
      >
        {children}
      </SelectedDaemonProvider>
    </AuthProvider>,
    backend,
  );
}

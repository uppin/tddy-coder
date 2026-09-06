import React, { useState } from "react";
import type { Client } from "@connectrpc/connect";
import type { Room } from "livekit-client";
import type { TokenService } from "../../gen/token_pb";
import { GhosttyTerminalLiveKit } from "../GhosttyTerminalLiveKit";
import { useLiveKitTerminalToken } from "./useLiveKitTerminalToken";
import { useIsMobile } from "../../hooks/useIsMobile";
import type { ToolShortcutDef } from "../../lib/toolShortcuts";
import type { LiveKitChromeStatus } from "../../lib/liveKitStatusPresentation";
import type { ByteDelta } from "./sessionRuntimeRegistry";

type TokenClient = Client<typeof TokenService>;

interface SessionLiveKitTerminalProps {
  livekitUrl: string;
  livekitRoom: string;
  livekitServerIdentity: string;
  tokenClient: TokenClient;
  /** Daemon session token + id, threaded to the terminal for the file drop upload feature. */
  sessionToken: string;
  sessionId: string;
  onDisconnect?: () => void;
  mobileShortcuts?: ToolShortcutDef[];
  /** Fired once with the session's connected LiveKit `Room` (see `GhosttyTerminalLiveKit.onRoom`). */
  onRoom?: (room: Room) => void;
  /** Called with a function that returns keyboard focus to this terminal (see
   *  `GhosttyTerminalLiveKit.onRegisterFocus`). Lets the runtime re-focus the terminal when its
   *  session is re-selected, without a click. */
  onRegisterFocus?: (focus: () => void) => void;
  /** Called with this terminal's text-insert function (see `GhosttyTerminalLiveKit.onRegisterInsertInput`),
   *  so the runtime can expose it to the inspector's Files-tab click/tap route. */
  onRegisterInsertInput?: (insertInput: (text: string) => void) => void;
  /** Fired when the underlying LiveKit room's connection status changes (connecting → connected, or
   *  → error). Lets the runtime cover the panes with a connection overlay until the room connects. */
  onConnectionStatusChange?: (status: LiveKitChromeStatus) => void;
  /** Fired per terminal I/O event (see `GhosttyTerminalLiveKit.onBytes`) so the runtime can account
   *  this session's byte traffic to its inspector counters. */
  onBytes?: (delta: ByteDelta) => void;
}

/**
 * Renders the same underlying Ghostty terminal used for Claude CLI's LiveKit-routed sessions
 * (`ConnectionScreen.tsx`'s `ConnectedTerminal`), for sessions attached over LiveKit rather than
 * the daemon's direct gRPC terminal stream (tddy-coder recipe sessions today; any remotely-routed
 * session tomorrow).
 */
export function SessionLiveKitTerminal({
  livekitUrl,
  livekitRoom,
  livekitServerIdentity,
  tokenClient,
  sessionToken,
  sessionId,
  onDisconnect,
  mobileShortcuts,
  onRoom,
  onRegisterFocus,
  onRegisterInsertInput,
  onConnectionStatusChange,
  onBytes,
}: SessionLiveKitTerminalProps) {
  const identity = useBrowserIdentityFor(livekitRoom, sessionId);
  const { token, ttlSeconds, getToken } = useLiveKitTerminalToken(tokenClient, livekitRoom, identity);
  const isMobile = useIsMobile();

  if (token === null || ttlSeconds === null) {
    // Initial token fetch in flight (or failed) — GhosttyTerminalLiveKit requires a token up front.
    return null;
  }

  return (
    <GhosttyTerminalLiveKit
      url={livekitUrl}
      token={token}
      getToken={getToken}
      ttlSeconds={ttlSeconds}
      roomName={livekitRoom}
      serverIdentity={livekitServerIdentity}
      connectionChromePlacement="none"
      hideStatusStrip
      sessionToken={sessionToken}
      sessionId={sessionId}
      showMobileKeyboard={isMobile}
      onRemoteSessionEnded={onDisconnect}
      mobileShortcuts={mobileShortcuts}
      onRoom={onRoom}
      onRegisterFocus={onRegisterFocus}
      onRegisterInsertInput={onRegisterInsertInput}
      onConnectionStatusChange={onConnectionStatusChange}
      onBytes={onBytes}
    />
  );
}

/**
 * A participant identity for this terminal's own join of `sessionId`'s room.
 *
 * The random suffix is not decoration: without it two joins that overlap — a remount before the
 * previous participant has been reaped, two tabs on one session — mint the *same* identity within
 * the same millisecond, and a LiveKit room that sees an identity twice drops one of the two. The
 * same reasoning, and the same shape, as `anObserverIdentity` in
 * `rpc/connections/livekit/sessionConnection.ts`.
 */
function aBrowserIdentityFor(sessionId: string): string {
  return `browser-${sessionId}-${Date.now()}-${Math.random().toString(36).slice(2, 10)}`;
}

/**
 * This browser tab's own participant identity for `roomName`.
 *
 * Stable while the terminal keeps watching the same room — the token is minted for it, and a
 * regenerated identity would mean a fresh join under a new participant — and fresh when the room
 * changes, because two rooms are two participants and a room that saw the same identity twice would
 * drop one of them.
 *
 * Held as state adjusted on a changed room, rather than written into refs while rendering: a render
 * that mutates a ref has already happened by the time React decides whether to keep it, so a
 * discarded or replayed render leaves the identity and the room it was minted for disagreeing —
 * and the mismatch surfaces as a token issued for the wrong room.
 *
 * Minted here rather than handed down with the attachment: it belongs to *this* terminal's own join,
 * which is a separate connection from the one the session's RPC travels over. Node 5 of the
 * `optional-livekit` stack folds this join into that connection, at which point the identity goes
 * with it.
 */
function useBrowserIdentityFor(roomName: string, sessionId: string): string {
  const [identity, setIdentity] = useState(() => aBrowserIdentityFor(sessionId));
  const [mintedFor, setMintedFor] = useState(roomName);
  if (mintedFor !== roomName) {
    setMintedFor(roomName);
    setIdentity(aBrowserIdentityFor(sessionId));
  }
  return identity;
}

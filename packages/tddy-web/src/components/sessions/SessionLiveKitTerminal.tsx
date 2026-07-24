import React from "react";
import type { Client } from "@connectrpc/connect";
import type { Room } from "livekit-client";
import type { TokenService } from "../../gen/token_pb";
import { GhosttyTerminalLiveKit } from "../GhosttyTerminalLiveKit";
import { useLiveKitTerminalToken } from "./useLiveKitTerminalToken";
import type { ToolShortcutDef } from "../../lib/toolShortcuts";
import type { LiveKitChromeStatus } from "../../lib/liveKitStatusPresentation";
import type { ByteDelta } from "./sessionRuntimeRegistry";

type TokenClient = Client<typeof TokenService>;

interface SessionLiveKitTerminalProps {
  livekitUrl: string;
  livekitRoom: string;
  livekitServerIdentity: string;
  /** This browser tab's own LiveKit identity, from `SessionAttachmentState`'s `connected-livekit` variant. */
  identity: string;
  tokenClient: TokenClient;
  onDisconnect?: () => void;
  mobileShortcuts?: ToolShortcutDef[];
  /** Fired once with the session's connected LiveKit `Room` (see `GhosttyTerminalLiveKit.onRoom`). */
  onRoom?: (room: Room) => void;
  /** Called with a function that returns keyboard focus to this terminal (see
   *  `GhosttyTerminalLiveKit.onRegisterFocus`). Lets the runtime re-focus the terminal when its
   *  session is re-selected, without a click. */
  onRegisterFocus?: (focus: () => void) => void;
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
  identity,
  tokenClient,
  onDisconnect,
  mobileShortcuts,
  onRoom,
  onRegisterFocus,
  onConnectionStatusChange,
  onBytes,
}: SessionLiveKitTerminalProps) {
  const { token, ttlSeconds, getToken } = useLiveKitTerminalToken(tokenClient, livekitRoom, identity);

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
      onRemoteSessionEnded={onDisconnect}
      mobileShortcuts={mobileShortcuts}
      onRoom={onRoom}
      onRegisterFocus={onRegisterFocus}
      onConnectionStatusChange={onConnectionStatusChange}
      onBytes={onBytes}
    />
  );
}

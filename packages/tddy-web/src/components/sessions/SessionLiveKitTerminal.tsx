import React from "react";
import type { Client } from "@connectrpc/connect";
import type { TokenService } from "../../gen/token_pb";
import { GhosttyTerminalLiveKit } from "../GhosttyTerminalLiveKit";
import { useLiveKitTerminalToken } from "./useLiveKitTerminalToken";
import type { ToolShortcutDef } from "../../lib/toolShortcuts";

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
    />
  );
}

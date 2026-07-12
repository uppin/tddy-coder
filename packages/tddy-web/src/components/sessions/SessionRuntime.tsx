import React, { useCallback, useRef } from "react";
import { createClient, type Client, type Transport } from "@connectrpc/connect";
import type { Room } from "livekit-client";
import { ConnectionService } from "../../gen/connection_pb";
import type { TokenService } from "../../gen/token_pb";
import { SessionLiveKitTerminal } from "./SessionLiveKitTerminal";
import { GrpcSessionTerminal } from "./GrpcSessionTerminal";
import { TerminalControlOverlay } from "./TerminalControlOverlay";
import { useTerminalControl } from "./useTerminalControl";
import type { SessionRuntimeState } from "./sessionRuntimeRegistry";
import type { ToolShortcutDef } from "../../lib/toolShortcuts";
import { cn } from "../../lib/utils";

type ConnectionClient = Client<typeof ConnectionService>;
type TokenClient = Client<typeof TokenService>;

export interface SessionRuntimeProps {
  /** This runtime's attached-session state (connection params + status). */
  runtime: SessionRuntimeState;
  /** True when this runtime is the focused (CSS-visible) one. Drives the control overlay + mobile
   *  shortcut overlay; backgrounded runtimes stay mounted but `display:none`. */
  focused: boolean;
  sessionToken: string;
  /** Owning daemon `ConnectionService` client — used for gRPC terminal I/O and as the fallback for
   *  the auto-claim-on-attach. Pass `null`/`undefined` until the owning daemon is reachable. */
  client?: ConnectionClient | null;
  /** Browser LiveKit-token client — required to render a LiveKit terminal. */
  tokenClient?: TokenClient;
  /** Shortcut presets — shown as the mobile shortcut overlay on the focused runtime only. */
  mobileShortcuts?: ToolShortcutDef[];
  /** Capture the runtime's connected LiveKit `Room` (for session-scoped RPC routing). */
  onSessionRoom?: (sessionId: string, room: Room) => void;
  /** Evict this runtime's terminal (e.g. remote session ended). */
  onSessionDisconnect?: (sessionId: string) => void;
  /** LiveKit transport factory — builds the session-scoped client transport for the explicit
   *  steal-claim (`ClaimTerminalControl`, session-participant routing). */
  liveKitFactory?: (room: Room, targetIdentity: string) => Transport;
  /** True when `liveKitFactory` is a test double that ignores its `room` argument — the common
   *  room is then an acceptable stand-in for the session's own room. */
  liveKitFactoryIsOverridden?: boolean;
  /** Shared common room — used as the session-room stand-in when the factory is overridden. */
  commonRoom?: Room | null;
}

/**
 * One mounted terminal + its own terminal-control lease for a single attached session.
 *
 * Each runtime owns its `useTerminalControl` hook — and therefore its own `controlTokenRef` — so
 * switching focus between sessions can never leak one session's control token into another
 * session's terminal input (the root cause of the "terminal controlled by another screen" failures
 * on fast session change). The focused runtime additionally carries the
 * `sessions-detail-terminal-container` marker and the `TerminalControlOverlay`.
 *
 * Feature: `docs/ft/web/session-drawer.md#fast-session-change`.
 */
export function SessionRuntime({
  runtime,
  focused,
  sessionToken,
  client,
  tokenClient,
  mobileShortcuts,
  onSessionRoom,
  onSessionDisconnect,
  liveKitFactory,
  liveKitFactoryIsOverridden = false,
  commonRoom = null,
}: SessionRuntimeProps) {
  // The runtime's own connected Room, captured via the terminal's `onRoom`. Stored in a ref (not
  // state) so capturing it does not re-render — it only feeds the lazy session-scoped client below.
  const roomRef = useRef<Room | null>(null);

  // Lazy session-scoped ConnectionService client (targets the coder participant
  // `daemon-{instanceId}-{sessionId}` = `runtime.livekitServerIdentity`). Used by the explicit
  // steal-claim so "Claim terminal" routes through the session participant. Built only for
  // `connected-livekit` runtimes; `null` otherwise (the daemon client is the fallback).
  const buildSessionClient = useCallback((): ConnectionClient | null => {
    if (runtime.status !== "connected-livekit") return null;
    const targetIdentity = runtime.livekitServerIdentity;
    if (!targetIdentity || !liveKitFactory) return null;
    const sessionRoom = roomRef.current ?? (liveKitFactoryIsOverridden ? commonRoom : null);
    if (!sessionRoom) return null;
    return createClient(ConnectionService, liveKitFactory(sessionRoom, targetIdentity));
  }, [
    runtime.status,
    runtime.livekitServerIdentity,
    liveKitFactory,
    liveKitFactoryIsOverridden,
    commonRoom,
  ]);

  // The runtime owns its own control lease + token. The auto-claim-on-attach uses the owning
  // daemon `client`; the explicit "Claim terminal" steal-claim routes through `buildSessionClient`.
  const { controlState, controlTokenRef, claim: claimControl } = useTerminalControl(
    runtime.sessionId,
    sessionToken,
    client ?? null,
    buildSessionClient,
  );

  const handleRoom = useCallback(
    (sessionRoom: Room) => {
      roomRef.current = sessionRoom;
      onSessionRoom?.(runtime.sessionId, sessionRoom);
    },
    [onSessionRoom, runtime.sessionId],
  );

  return (
    <div
      data-testid={`sessions-runtime-terminal-${runtime.sessionId}`}
      className={cn("absolute inset-0 h-full w-full", focused ? "" : "hidden")}
      aria-hidden={!focused}
    >
      {runtime.status === "connected-livekit" && tokenClient && runtime.livekitRoom && (
        <div className="h-full w-full">
          <SessionLiveKitTerminal
            livekitUrl={runtime.livekitUrl ?? ""}
            livekitRoom={runtime.livekitRoom}
            livekitServerIdentity={runtime.livekitServerIdentity ?? ""}
            identity={runtime.identity ?? ""}
            tokenClient={tokenClient}
            onDisconnect={() => onSessionDisconnect?.(runtime.sessionId)}
            mobileShortcuts={focused ? mobileShortcuts : undefined}
            onRoom={handleRoom}
          />
        </div>
      )}
      {runtime.status === "connected-livekit" && !tokenClient && (
        <div className="h-full w-full text-xs text-muted-foreground p-4">
          Terminal connected to {runtime.livekitRoom}
        </div>
      )}
      {runtime.status === "connected-grpc" && client && (
        <div className="h-full w-full">
          <GrpcSessionTerminal
            sessionId={runtime.sessionId}
            sessionToken={sessionToken}
            client={client}
            controlToken={controlTokenRef.current}
            onDisconnect={() => onSessionDisconnect?.(runtime.sessionId)}
            mobileShortcuts={focused ? mobileShortcuts : undefined}
          />
        </div>
      )}
      {focused && (
        // The focused runtime carries the terminal-control mutex overlay and the
        // `sessions-detail-terminal-container` marker (existing acceptance contract). The overlay is
        // only rendered when a control client is available — without one the lease is not being
        // managed, so the terminal stays interactive (no spurious "Claim terminal" CTA).
        // `pointer-events-none` lets clicks reach the terminal below when no overlay is showing;
        // the overlay itself re-enables pointer events.
        <div data-testid="sessions-detail-terminal-container" className="absolute inset-0 pointer-events-none">
          {client && (
            <TerminalControlOverlay
              isController={controlState.isController}
              holderScreenId={controlState.holderScreenId}
              onClaim={claimControl}
            />
          )}
        </div>
      )}
    </div>
  );
}

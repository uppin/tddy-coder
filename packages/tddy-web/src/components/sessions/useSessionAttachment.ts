/**
 * Attaching to a session, and holding the connection that attach produced.
 *
 * The daemon's `ConnectSession` / `ResumeSession` reply is read into a transport-neutral
 * {@link SessionAttachmentHint} (`rpc/connections/sessionAttachment`), and the host connection opens
 * the session over whichever wire it reaches it by. What used to be two states here —
 * `connected-livekit`, carrying four LiveKit fields, and a degraded `connected-grpc` — is one
 * `connected` state carrying a {@link SessionConnection}, so no consumer branches on the wire.
 *
 * The hint is published alongside the state because a room-backed session still has surfaces that
 * join its room for themselves: the terminal (`SessionLiveKitTerminal`) and the chat presenter
 * (`usePresenterLiveKitRoom`). Folding those into the connection is node 5's; until then they read
 * the routing from here rather than re-deriving it from a reply nobody kept.
 *
 * PRD: `docs/dev/1-WIP/2026-09-05-optional-livekit-session-connection-prd.md`.
 */

import { useCallback, useEffect, useRef, useState } from "react";
import type { Client } from "@connectrpc/connect";
import { ConnectionService } from "../../gen/connection_pb";
import { attachmentHintFromReply } from "../../rpc/connections/sessionAttachment";
import type { SessionAttachmentHint, SessionAttachmentState } from "../../rpc/connections/session";
import type { HostConnection } from "../../rpc/connections/types";

export type { SessionAttachmentState } from "../../rpc/connections/session";

type ConnectionClient = Client<typeof ConnectionService>;

/**
 * One attachment: what state it is in, and the routing it was opened with.
 *
 * The two travel together so that restoring an attachment — the drawer's fast-path select, which
 * re-focuses a runtime that is already mounted — restores both, rather than reconnecting a session
 * whose connection is still open just to learn its room again.
 */
export interface SessionAttachment {
  readonly state: SessionAttachmentState;
  /** How the daemon said to reach this session; `null` unless {@link state} is connected. */
  readonly hint: SessionAttachmentHint | null;
}

const NOT_ATTACHED: SessionAttachment = { state: { status: "idle" }, hint: null };

export interface UseSessionAttachmentResult {
  state: SessionAttachmentState;
  /** The routing behind {@link state}'s connection; `null` unless connected. */
  hint: SessionAttachmentHint | null;
  connectSession(sessionId: string, sessionToken: string, host: HostConnection): Promise<void>;
  resumeSession(sessionId: string, sessionToken: string, host: HostConnection): Promise<void>;
  deleteSession(
    sessionId: string,
    sessionToken: string,
    client: ConnectionClient,
  ): Promise<void>;
  signalSession(
    sessionId: string,
    signal: number,
    sessionToken: string,
    client: ConnectionClient,
  ): Promise<void>;
  /** Restore an already-open attachment without an RPC round-trip — used by the screen's fast-path
   *  select (switching focus to a session whose runtime is already mounted in the registry). */
  restore(attachment: SessionAttachment): void;
  reset(): void;
}

export function useSessionAttachment(): UseSessionAttachmentResult {
  const [attachment, setAttachment] = useState<SessionAttachment>(NOT_ATTACHED);

  // Whether this hook's owner is still mounted. An attach is an in-flight RPC, and the connection
  // it produces is opened *after* the reply lands — which can be after the screen that asked for it
  // has gone. Nothing downstream ever sees such a connection, so nothing downstream can close one:
  // it is opened here and it has to be released here.
  const mounted = useRef(true);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);

  /**
   * Attach `sessionId` on `host` with whichever reply the caller's RPC produced.
   *
   * The reply is turned into a hint and the host opens the session over it — the one place the
   * LiveKit fields of the reply are read, and the branch (`resp.livekitRoom !== ""`) that used to
   * decide which of two statuses every consumer would then have to handle.
   */
  const attach = useCallback(
    async (
      sessionId: string,
      host: HostConnection,
      call: (client: ConnectionClient) => Promise<{
        livekitRoom: string;
        livekitUrl: string;
        livekitServerIdentity: string;
      }>,
    ) => {
      setAttachment({ state: { status: "connecting", sessionId }, hint: null });
      try {
        const reply = await call(host.clientFor(ConnectionService));
        const hint = attachmentHintFromReply(sessionId, reply);
        const connection = host.openSession(sessionId, hint);
        if (!mounted.current) {
          // The reply outlived its screen. The connection is already open — a LiveKit one has
          // started joining a room — and the registry that would otherwise own it will never be
          // told about this one.
          connection.close();
          return;
        }
        setAttachment({ state: { status: "connected", connection }, hint });
      } catch (err) {
        const message = err instanceof Error ? err.message : String(err);
        setAttachment({ state: { status: "error", error: message }, hint: null });
      }
    },
    [],
  );

  const connectSession = useCallback(
    (sessionId: string, sessionToken: string, host: HostConnection) =>
      attach(sessionId, host, (client) => client.connectSession({ sessionToken, sessionId })),
    [attach],
  );

  const resumeSession = useCallback(
    (sessionId: string, sessionToken: string, host: HostConnection) =>
      attach(sessionId, host, (client) => client.resumeSession({ sessionToken, sessionId })),
    [attach],
  );

  const deleteSession = useCallback(
    async (sessionId: string, sessionToken: string, client: ConnectionClient) => {
      try {
        await client.deleteSession({ sessionToken, sessionId });
        setAttachment(NOT_ATTACHED);
      } catch (err) {
        const message = err instanceof Error ? err.message : String(err);
        setAttachment({ state: { status: "error", error: message }, hint: null });
      }
    },
    [],
  );

  const signalSession = useCallback(
    async (sessionId: string, signal: number, sessionToken: string, client: ConnectionClient) => {
      try {
        await client.signalSession({ sessionToken, sessionId, signal });
      } catch (err) {
        const message = err instanceof Error ? err.message : String(err);
        setAttachment({ state: { status: "error", error: message }, hint: null });
      }
    },
    [],
  );

  const restore = useCallback((next: SessionAttachment) => {
    setAttachment(next);
  }, []);

  const reset = useCallback(() => {
    setAttachment(NOT_ATTACHED);
  }, []);

  return {
    state: attachment.state,
    hint: attachment.hint,
    connectSession,
    resumeSession,
    deleteSession,
    signalSession,
    restore,
    reset,
  };
}

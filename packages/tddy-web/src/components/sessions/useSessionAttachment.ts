import { useState, useCallback } from "react";
import type { Client } from "@connectrpc/connect";
import type { ConnectionService } from "../../gen/connection_pb";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export type SessionAttachmentState =
  | { status: "idle" }
  | { status: "connecting" }
  | {
      status: "connected-livekit";
      sessionId: string;
      livekitRoom: string;
      livekitUrl: string;
      livekitServerIdentity: string;
      /** This browser tab's own LiveKit identity for joining the room (distinct from `livekitServerIdentity`). */
      identity: string;
    }
  | { status: "connected-grpc"; sessionId: string }
  | { status: "error"; error: string };

type ConnectionClient = Client<typeof ConnectionService>;

export interface UseSessionAttachmentResult {
  state: SessionAttachmentState;
  connectSession(
    sessionId: string,
    sessionToken: string,
    client: ConnectionClient,
  ): Promise<void>;
  resumeSession(
    sessionId: string,
    sessionToken: string,
    client: ConnectionClient,
  ): Promise<void>;
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
  /** Restore the attachment to an already-connected session's state without an RPC round-trip —
   *  used by the screen's fast-path select (switching focus to a session whose runtime is already
   *  mounted in the registry). Mirrors `connectSession`'s terminal state but skips the network hop. */
  restore(state: SessionAttachmentState): void;
  reset(): void;
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

type AttachResponse = { livekitRoom: string; livekitUrl: string; livekitServerIdentity: string };

function attachmentStateFromResponse(resp: AttachResponse, sessionId: string): SessionAttachmentState {
  if (resp.livekitRoom !== "") {
    return {
      status: "connected-livekit",
      sessionId,
      livekitRoom: resp.livekitRoom,
      livekitUrl: resp.livekitUrl,
      livekitServerIdentity: resp.livekitServerIdentity,
      identity: `browser-${sessionId}-${Date.now()}`,
    };
  }
  return { status: "connected-grpc", sessionId };
}

// ---------------------------------------------------------------------------
// Hook
// ---------------------------------------------------------------------------

export function useSessionAttachment(): UseSessionAttachmentResult {
  const [state, setState] = useState<SessionAttachmentState>({ status: "idle" });

  const connectSession = useCallback(
    async (sessionId: string, sessionToken: string, client: ConnectionClient) => {
      setState({ status: "connecting" });
      try {
        const resp = await client.connectSession({ sessionToken, sessionId });
        setState(attachmentStateFromResponse(resp, sessionId));
      } catch (err) {
        const message = err instanceof Error ? err.message : String(err);
        setState({ status: "error", error: message });
      }
    },
    [],
  );

  const resumeSession = useCallback(
    async (sessionId: string, sessionToken: string, client: ConnectionClient) => {
      setState({ status: "connecting" });
      try {
        const resp = await client.resumeSession({ sessionToken, sessionId });
        setState(attachmentStateFromResponse(resp, sessionId));
      } catch (err) {
        const message = err instanceof Error ? err.message : String(err);
        setState({ status: "error", error: message });
      }
    },
    [],
  );

  const deleteSession = useCallback(
    async (sessionId: string, sessionToken: string, client: ConnectionClient) => {
      try {
        await client.deleteSession({ sessionToken, sessionId });
        setState({ status: "idle" });
      } catch (err) {
        const message = err instanceof Error ? err.message : String(err);
        setState({ status: "error", error: message });
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
        setState({ status: "error", error: message });
      }
    },
    [],
  );

  const restore = useCallback((state: SessionAttachmentState) => {
    setState(state);
  }, []);

  const reset = useCallback(() => {
    setState({ status: "idle" });
  }, []);

  return { state, connectSession, resumeSession, deleteSession, signalSession, restore, reset };
}

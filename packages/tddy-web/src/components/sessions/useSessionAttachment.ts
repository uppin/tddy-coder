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

  const reset = useCallback(() => {
    setState({ status: "idle" });
  }, []);

  return { state, connectSession, resumeSession, reset };
}

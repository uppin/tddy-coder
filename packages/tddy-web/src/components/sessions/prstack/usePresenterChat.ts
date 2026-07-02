import { useEffect, useMemo, useRef, useState } from "react";
import { create } from "@bufbuild/protobuf";
import { createClient } from "@connectrpc/connect";
import type { Room } from "livekit-client";
import { AsyncQueue } from "tddy-livekit-web";
import {
  useLiveKitTransportFactory,
  useLiveKitTransportFactoryIsOverridden,
} from "../../../rpc/transportProvider";
import {
  ClientMessageSchema,
  TddyRemote,
  type AppModeProto,
  type ClientMessage,
  type ServerMessage,
} from "../../../gen/tddy/v1/remote_pb";

type AppModeCase = AppModeProto["variant"]["case"];

export interface ChatMessage {
  key: string;
  text: string;
  from: "user" | "agent";
}

export interface UsePresenterChatResult {
  messages: ChatMessage[];
  /** Enqueues `text` onto the open stream. Returns `false` (and enqueues nothing) when there is no live client to send over, or when the presenter's own participant is no longer in the room. */
  sendPrompt: (text: string) => boolean;
  /** Set when the presenter stream fails after a client was already built — `null` otherwise. */
  streamError: string | null;
  /** Set when the most recent `sendPrompt` call returned `false` — explains why. Cleared on the next successful send. */
  sendError: string | null;
}

/**
 * Owns the `TddyRemote.Stream` bidirectional RPC for the PR-Stack Chat Screen — a thin UI over
 * the session's remote Presenter protocol (docs/ft/web/session-drawer.md § PR-Stack Chat
 * Screen). Inbound `AgentOutput` events become chat bubbles; `sendPrompt` writes a `ClientMessage`
 * intent onto the same open stream.
 *
 * The presenter only starts the workflow on a `SubmitFeatureInput` intent (sent while in
 * `AppMode::FeatureInput`); `QueuePrompt` is for nudging an already-running workflow and is a
 * no-op otherwise (pushes into an inbox nobody reads until a workflow is running — see
 * `tddy-core/src/presenter/presenter_impl.rs`, `QueuePrompt` handler). The server does not send
 * a mode snapshot to a newly-connected stream (`TddyRemoteService::stream` subscribes to future
 * broadcasts only), so this hook can't simply wait for an authoritative "current mode" on
 * connect. It tracks mode two ways: authoritatively from any `ModeChanged` event that does
 * arrive, and as a fallback, treats the very first message sent on a fresh connection as
 * `SubmitFeatureInput` (matching every recipe's actual fresh-session behavior) and every
 * subsequent message as `QueuePrompt`.
 *
 * `room` / `serverIdentity` select the LiveKit transport target. The production default
 * transport (`createDefaultLiveKitTransport`) requires a genuinely connected `Room` and
 * crashes without one, so this hook only builds a client when `room` is non-null — *unless*
 * a test double has overridden the factory (`RpcTransportProvider liveKitFactory`), since
 * those are free to ignore `room` entirely (e.g. routing to an in-memory RPC backend). Without
 * a client, `sendPrompt` refuses to send and returns `false`.
 *
 * A live client is not enough on its own: `LiveKitTransport.publishData` addresses the daemon's
 * participant (`serverIdentity`) via `destinationIdentities` on a reliable data-channel send,
 * which LiveKit accepts with no delivery/presence signal at all — a message aimed at an
 * identity that has dropped off the room (e.g. the daemon-side process lost its own LiveKit
 * connection) vanishes with no error, and the stream's response side hangs forever waiting for
 * a reply that will never come. `sendPrompt` therefore checks `room.remoteParticipants` for
 * `serverIdentity` before treating a send as accepted.
 *
 * TODO: thread a real connected LiveKit `Room` for the orchestrator session through from
 * `SessionsDrawerScreen` once a pr-stack session has its own independent attach flow (today
 * `PrStackScreen` renders before any terminal attachment exists, per the "not gated on
 * attachment.status" requirement) — until then chat has no client and cannot send/receive in
 * production.
 */
export function usePresenterChat(
  room: Room | null,
  serverIdentity: string,
): UsePresenterChatResult {
  const liveKitFactory = useLiveKitTransportFactory();
  const factoryIsOverridden = useLiveKitTransportFactoryIsOverridden();
  const canBuildClient = room !== null || factoryIsOverridden;

  // Stable client for the lifetime of this room/identity pair — a new room or identity
  // (e.g. switching sessions) tears down the old stream and opens a fresh one. `null` when
  // there is no room yet and no test double to route through instead.
  const client = useMemo(() => {
    if (!canBuildClient) return null;
    const transport = liveKitFactory(room as Room, serverIdentity);
    return createClient(TddyRemote, transport);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [liveKitFactory, canBuildClient, room, serverIdentity]);

  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [streamError, setStreamError] = useState<string | null>(null);
  const [sendError, setSendError] = useState<string | null>(null);
  const queueRef = useRef<AsyncQueue<ClientMessage> | null>(null);
  // Authoritative mode from the last ModeChanged event seen, if any.
  const modeRef = useRef<AppModeCase | null>(null);
  // Fallback used while no ModeChanged event has arrived yet: every recipe's fresh session
  // starts in FeatureInput, so the first message on a new connection targets that.
  const hasSentRef = useRef(false);
  // Key counter for the operator's own echoed messages — independent of the read loop's
  // `index`, which only counts inbound server messages.
  const sentIndexRef = useRef(0);

  useEffect(() => {
    setMessages([]);
    setStreamError(null);
    setSendError(null);
    modeRef.current = null;
    hasSentRef.current = false;
    sentIndexRef.current = 0;
    if (!client) {
      queueRef.current = null;
      return;
    }

    const queue = new AsyncQueue<ClientMessage>();
    queueRef.current = queue;
    let cancelled = false;
    let index = 0;

    (async () => {
      try {
        for await (const serverMessage of client.stream(queue)) {
          if (cancelled) break;
          if (serverMessage.event.case === "modeChanged") {
            modeRef.current = serverMessage.event.value.mode?.variant.case ?? null;
          }
          const chat = chatMessageFromServerMessage(serverMessage, index);
          if (chat) {
            index += 1;
            setMessages((prev) => [...prev, chat]);
          }
        }
      } catch (err) {
        if (!cancelled) {
          console.debug("[usePresenterChat] stream error", err);
          setStreamError(err instanceof Error ? err.message : String(err));
        }
      }
    })();

    return () => {
      cancelled = true;
      queue.close();
      if (queueRef.current === queue) {
        queueRef.current = null;
      }
    };
  }, [client]);

  const sendPrompt = (text: string): boolean => {
    if (!queueRef.current) {
      setSendError("Message not sent — no connection to the presenter yet.");
      return false;
    }
    if (room && !room.remoteParticipants.has(serverIdentity)) {
      setSendError("Message not sent — the presenter is not connected.");
      return false;
    }
    const startsFeature = modeRef.current
      ? modeRef.current === "featureInput"
      : !hasSentRef.current;
    hasSentRef.current = true;
    const clientMessage = startsFeature
      ? create(ClientMessageSchema, { intent: { case: "submitFeatureInput", value: { text } } })
      : create(ClientMessageSchema, { intent: { case: "queuePrompt", value: { text } } });
    queueRef.current.enqueue(clientMessage);
    setSendError(null);
    const sentIndex = sentIndexRef.current;
    sentIndexRef.current += 1;
    setMessages((prev) => [...prev, { key: `user-sent-${sentIndex}`, text, from: "user" }]);
    return true;
  };

  return { messages, sendPrompt, streamError, sendError };
}

/** Map a `ServerMessage` `PresenterEvent` to a chat bubble. Non-chat events yield `null`. */
function chatMessageFromServerMessage(msg: ServerMessage, index: number): ChatMessage | null {
  switch (msg.event.case) {
    case "agentOutput":
      return { key: `agent-output-${index}`, text: msg.event.value.text, from: "agent" };
    default:
      return null;
  }
}

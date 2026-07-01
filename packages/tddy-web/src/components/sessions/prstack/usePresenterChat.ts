import { useEffect, useMemo, useRef, useState } from "react";
import { create } from "@bufbuild/protobuf";
import { createClient } from "@connectrpc/connect";
import type { Room } from "livekit-client";
import { AsyncQueue } from "tddy-livekit-web";
import { useLiveKitTransportFactory } from "../../../rpc/transportProvider";
import {
  ClientMessageSchema,
  TddyRemote,
  type ClientMessage,
  type ServerMessage,
} from "../../../gen/tddy/v1/remote_pb";

export interface ChatMessage {
  key: string;
  text: string;
}

export interface UsePresenterChatResult {
  messages: ChatMessage[];
  sendPrompt: (text: string) => void;
}

/**
 * Owns the `TddyRemote.Stream` bidirectional RPC for the PR-Stack Chat Screen — a thin UI over
 * the session's remote Presenter protocol (docs/ft/web/session-drawer.md § PR-Stack Chat
 * Screen). Inbound `AgentOutput` events become chat bubbles; `sendPrompt` writes a
 * `QueuePrompt` intent onto the same open stream.
 *
 * `room` / `serverIdentity` select the LiveKit transport target — mirrors the pattern in
 * `GhosttyTerminalLiveKit` (`useLiveKitTransportFactory()` + `createClient`), since the
 * generic `useLiveKitClient` hook requires a non-null `Room` up front and the presenter
 * connection for an orchestrator session may not have one yet.
 *
 * TODO: thread a real connected LiveKit `Room` for the orchestrator session through from
 * `SessionsDrawerScreen` once a pr-stack session has its own independent attach flow (today
 * `PrStackScreen` renders before any terminal attachment exists, per the "not gated on
 * attachment.status" requirement) — until then this hook works over whatever transport the
 * factory produces for the given identity. Land alongside that: the stream read loop's catch
 * block below only `console.debug`s a transport failure today (chat just stops updating with
 * no operator-visible signal) — surface a real error/disconnected state in `PrStackChat` once
 * there's a genuine connection to report on.
 */
export function usePresenterChat(
  room: Room | null,
  serverIdentity: string,
): UsePresenterChatResult {
  const liveKitFactory = useLiveKitTransportFactory();
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const queueRef = useRef<AsyncQueue<ClientMessage> | null>(null);

  // Stable client for the lifetime of this room/identity pair — a new room or identity
  // (e.g. switching sessions) tears down the old stream and opens a fresh one.
  const client = useMemo(() => {
    const transport = liveKitFactory(room as Room, serverIdentity);
    return createClient(TddyRemote, transport);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [liveKitFactory, room, serverIdentity]);

  useEffect(() => {
    setMessages([]);
    const queue = new AsyncQueue<ClientMessage>();
    queueRef.current = queue;
    let cancelled = false;
    let index = 0;

    (async () => {
      try {
        for await (const serverMessage of client.stream(queue)) {
          if (cancelled) break;
          const chat = chatMessageFromServerMessage(serverMessage, index);
          if (chat) {
            index += 1;
            setMessages((prev) => [...prev, chat]);
          }
        }
      } catch (err) {
        if (!cancelled) {
          console.debug("[usePresenterChat] stream error", err);
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

  const sendPrompt = (text: string) => {
    queueRef.current?.enqueue(
      create(ClientMessageSchema, { intent: { case: "queuePrompt", value: { text } } }),
    );
  };

  return { messages, sendPrompt };
}

/** Map a `ServerMessage` `PresenterEvent` to a chat bubble. Non-chat events yield `null`. */
function chatMessageFromServerMessage(msg: ServerMessage, index: number): ChatMessage | null {
  switch (msg.event.case) {
    case "agentOutput":
      return { key: `agent-output-${index}`, text: msg.event.value.text };
    default:
      return null;
  }
}

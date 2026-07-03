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
} from "../../../gen/tddy/v1/remote_pb";
import { tddyDebug } from "../../../lib/debugMask";

/** Presenter-stream diagnostics. Enable in DevTools: `localStorage.debug = 'tddy:presenter:*'`
 *  (or via the daemon `debug` config). Traces stream open/close, every inbound Presenter event,
 *  and every outbound intent — so an empty chat can be pinned to "stream never opened" vs
 *  "opened but no agentOutput arrived" vs "arrived but not rendered". */
const dbg = tddyDebug("tddy:presenter:stream");

type AppModeCase = AppModeProto["variant"]["case"];

export interface ChatMessage {
  key: string;
  text: string;
  from: "user" | "agent" | "goal" | "activity";
}

export interface PendingQuestionOption {
  label: string;
  description: string;
}

export interface PendingQuestion {
  kind: "select" | "multiSelect";
  header: string;
  question: string;
  options: PendingQuestionOption[];
  allowOther: boolean;
}

export interface UsePresenterChatResult {
  messages: ChatMessage[];
  /** Enqueues `text` onto the open stream. Returns `false` (and enqueues nothing) when there is no live client to send over, or when the presenter's own participant is no longer in the room. */
  sendPrompt: (text: string) => boolean;
  /** The clarification question the presenter is currently blocked on (`AppMode::Select` /
   *  `AppMode::MultiSelect`), or `null` when the workflow isn't awaiting an answer. */
  pendingQuestion: PendingQuestion | null;
  /** Enqueues an `AnswerSelect` intent for the given option index. Same connection guards as `sendPrompt`. */
  answerSelect: (index: number) => boolean;
  /** Enqueues an `AnswerOther` intent with a custom typed answer. Same connection guards as `sendPrompt`. */
  answerOther: (text: string) => boolean;
  /** Enqueues an `AnswerMultiSelect` intent with the checked option indices (and optional custom text). Same connection guards as `sendPrompt`. */
  answerMultiSelect: (indices: number[], other?: string) => boolean;
  /** Set when the presenter stream fails after a client was already built — `null` otherwise. */
  streamError: string | null;
  /** Set when the most recent `sendPrompt` call returned `false` — explains why. Cleared on the next successful send. */
  sendError: string | null;
  /** Set when the workflow reports failure (`WorkflowComplete { ok: false }`) over the stream — the server-side failure message. `null` otherwise. */
  workflowError: string | null;
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
    if (!canBuildClient) {
      dbg(
        "no client: canBuildClient=false (room=%o, factoryOverridden=%o) — stream will NOT open",
        room !== null,
        factoryIsOverridden,
      );
      return null;
    }
    dbg("building client: room=%o serverIdentity=%o factoryOverridden=%o", room !== null, serverIdentity, factoryIsOverridden);
    const transport = liveKitFactory(room as Room, serverIdentity);
    return createClient(TddyRemote, transport);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [liveKitFactory, canBuildClient, room, serverIdentity]);

  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [pendingQuestion, setPendingQuestion] = useState<PendingQuestion | null>(null);
  const [streamError, setStreamError] = useState<string | null>(null);
  const [sendError, setSendError] = useState<string | null>(null);
  const [workflowError, setWorkflowError] = useState<string | null>(null);
  const queueRef = useRef<AsyncQueue<ClientMessage> | null>(null);
  // Authoritative mode from the last ModeChanged event seen, if any.
  const modeRef = useRef<AppModeCase | null>(null);
  // Fallback used while no ModeChanged event has arrived yet: every recipe's fresh session
  // starts in FeatureInput, so the first message on a new connection targets that.
  const hasSentRef = useRef(false);
  // Key counter for the operator's own echoed messages.
  const sentIndexRef = useRef(0);
  // Key counter for goal/activity system bubbles.
  const systemKeyRef = useRef(0);

  // Streaming agent-output merge — mirrors tddy-core's `AgentOutputActivityLogMerge` (the same
  // reconciliation the TUI View applies). The presenter broadcasts raw `AgentOutput` chunks: token
  // deltas as they stream, plus (for some backends) the whole line again as a snapshot after its
  // `\n`. We accumulate deltas into ONE growing agent bubble, finalize it on `\n`, and dedup the
  // repeated snapshot — so a sentence renders as a single line streamed token-by-token, not one
  // bubble per token and with no duplicated compound sentence. `messagesRef` is the source of truth
  // for both agent and user bubbles; `setMessages` just mirrors it for render.
  const messagesRef = useRef<ChatMessage[]>([]);
  const agentBufferRef = useRef("");
  const agentPartialActiveRef = useRef(false);
  const agentKeyRef = useRef(0);

  const lastAgentIndex = (): number => {
    const m = messagesRef.current;
    return m.length > 0 && m[m.length - 1].from === "agent" ? m.length - 1 : -1;
  };
  const setAgentText = (i: number, text: string) => {
    messagesRef.current[i] = { ...messagesRef.current[i], text };
  };
  const pushAgentLine = (text: string) => {
    messagesRef.current.push({ key: `agent-line-${agentKeyRef.current++}`, text, from: "agent" });
  };
  /** A finalized line (delta buffer flushed on `\n`): replace the in-progress partial row, else
   *  append — but drop a re-emitted snapshot identical to the line already shown. */
  const finalizeAgentLine = (line: string) => {
    if (line.length === 0) return;
    if (agentPartialActiveRef.current) {
      const i = lastAgentIndex();
      if (i >= 0) {
        setAgentText(i, line);
        agentPartialActiveRef.current = false;
        return;
      }
      agentPartialActiveRef.current = false;
    }
    const i = lastAgentIndex();
    if (i >= 0 && messagesRef.current[i].text === line) return;
    pushAgentLine(line);
  };
  /** The still-growing (no newline yet) buffer: keep updating the one partial agent row. */
  const syncAgentPartial = (buffer: string) => {
    if (buffer.length === 0) return;
    if (agentPartialActiveRef.current) {
      const i = lastAgentIndex();
      if (i >= 0) {
        setAgentText(i, buffer);
        return;
      }
    }
    pushAgentLine(buffer);
    agentPartialActiveRef.current = true;
  };
  /** Apply one raw `AgentOutput` chunk the same way `AgentOutputActivityLogMerge::apply_chunk` does. */
  const appendAgentChunk = (text: string) => {
    // split_inclusive('\n'): each part keeps its trailing newline (if any).
    const parts = text.match(/[^\n]*\n|[^\n]+/g) ?? [];
    for (const part of parts) {
      if (part.endsWith("\n")) {
        agentBufferRef.current += part.slice(0, -1);
        const line = agentBufferRef.current;
        agentBufferRef.current = "";
        finalizeAgentLine(line);
      } else {
        agentBufferRef.current += part;
      }
    }
    syncAgentPartial(agentBufferRef.current);
    setMessages(messagesRef.current.slice());
  };

  useEffect(() => {
    setMessages([]);
    setPendingQuestion(null);
    setStreamError(null);
    setSendError(null);
    setWorkflowError(null);
    modeRef.current = null;
    hasSentRef.current = false;
    sentIndexRef.current = 0;
    systemKeyRef.current = 0;
    messagesRef.current = [];
    agentBufferRef.current = "";
    agentPartialActiveRef.current = false;
    agentKeyRef.current = 0;
    if (!client) {
      dbg("effect: no client — stream not opened (chat will stay empty)");
      queueRef.current = null;
      return;
    }

    const queue = new AsyncQueue<ClientMessage>();
    queueRef.current = queue;
    let cancelled = false;
    let eventCount = 0;

    // Open the stream eagerly. The LiveKit transport only publishes the stream-open frame (which
    // makes the server run `TddyRemote.Stream` -> `connect_view` -> replay snapshot + subscribe to
    // live events) on the FIRST enqueued client message. Without this, a passive viewer who never
    // sends anything gets no events at all. An empty `ClientMessage` (no intent) triggers the open
    // and is ignored server-side (`client_message_to_intent` returns None). It does not count as the
    // operator's first prompt, so `sendPrompt`'s SubmitFeatureInput-vs-QueuePrompt logic is unchanged.
    queue.enqueue(create(ClientMessageSchema, {}));

    dbg("opening TddyRemote.Stream (server identity=%o) — enqueued eager open frame", serverIdentity);
    (async () => {
      try {
        for await (const serverMessage of client.stream(queue)) {
          if (cancelled) break;
          eventCount += 1;
          const kind = serverMessage.event.case;
          if (kind === "agentOutput") {
            dbg("recv #%d agentOutput (%d chars): %o", eventCount, serverMessage.event.value.text.length, serverMessage.event.value.text.slice(0, 120));
          } else if (kind === "modeChanged") {
            const variant = serverMessage.event.value.mode?.variant;
            modeRef.current = variant?.case ?? null;
            dbg("recv #%d modeChanged -> %o", eventCount, modeRef.current);
            if (variant?.case === "select" || variant?.case === "multiSelect") {
              const question = variant.value.question;
              setPendingQuestion(
                question
                  ? {
                      kind: variant.case,
                      header: question.header,
                      question: question.question,
                      options: question.options.map((o) => ({
                        label: o.label,
                        description: o.description,
                      })),
                      allowOther: question.allowOther,
                    }
                  : null,
              );
            } else {
              setPendingQuestion(null);
            }
          } else if (kind === "workflowComplete") {
            const complete = serverMessage.event.value;
            if (!complete.ok) {
              dbg("recv #%d workflowComplete FAILED: %o", eventCount, complete.message);
              setWorkflowError(complete.message || "Workflow failed.");
            } else {
              dbg("recv #%d workflowComplete ok", eventCount);
            }
          } else if (kind === "goalStarted") {
            const goal = serverMessage.event.value.goal;
            dbg("recv #%d goalStarted -> %o", eventCount, goal);
            messagesRef.current.push({ key: `system-${systemKeyRef.current++}`, text: goal, from: "goal" });
            setMessages(messagesRef.current.slice());
          } else if (kind === "activityLogged") {
            const activity = serverMessage.event.value;
            dbg("recv #%d activityLogged -> %o", eventCount, activity);
            if (activity.kind !== "UserPrompt") {
              messagesRef.current.push({
                key: `system-${systemKeyRef.current++}`,
                text: activity.text,
                from: "activity",
              });
              setMessages(messagesRef.current.slice());
            }
          } else {
            dbg("recv #%d %s", eventCount, kind);
          }
          if (kind === "agentOutput") {
            appendAgentChunk(serverMessage.event.value.text);
          }
        }
        dbg("stream closed after %d event(s) (cancelled=%o)", eventCount, cancelled);
      } catch (err) {
        if (!cancelled) {
          dbg("stream error after %d event(s): %o", eventCount, err);
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
      dbg("sendPrompt refused: no open stream/queue (%o chars)", text.length);
      setSendError("Message not sent — no connection to the presenter yet.");
      return false;
    }
    if (room && !room.remoteParticipants.has(serverIdentity)) {
      dbg("sendPrompt refused: presenter %o not in room participants", serverIdentity);
      setSendError("Message not sent — the presenter is not connected.");
      return false;
    }
    const startsFeature = modeRef.current
      ? modeRef.current === "featureInput"
      : !hasSentRef.current;
    hasSentRef.current = true;
    dbg("sendPrompt: intent=%s (%d chars)", startsFeature ? "submitFeatureInput" : "queuePrompt", text.length);
    const clientMessage = startsFeature
      ? create(ClientMessageSchema, { intent: { case: "submitFeatureInput", value: { text } } })
      : create(ClientMessageSchema, { intent: { case: "queuePrompt", value: { text } } });
    queueRef.current.enqueue(clientMessage);
    setSendError(null);
    messagesRef.current.push({ key: `user-sent-${sentIndexRef.current++}`, text, from: "user" });
    setMessages(messagesRef.current.slice());
    return true;
  };

  const canSend = (): boolean => {
    if (!queueRef.current) {
      dbg("answer refused: no open stream/queue");
      setSendError("Message not sent — no connection to the presenter yet.");
      return false;
    }
    if (room && !room.remoteParticipants.has(serverIdentity)) {
      dbg("answer refused: presenter %o not in room participants", serverIdentity);
      setSendError("Message not sent — the presenter is not connected.");
      return false;
    }
    return true;
  };

  const answerSelect = (index: number): boolean => {
    if (!canSend()) return false;
    dbg("answerSelect: index=%d", index);
    queueRef.current!.enqueue(
      create(ClientMessageSchema, { intent: { case: "answerSelect", value: { index } } }),
    );
    setSendError(null);
    const label = pendingQuestion?.options[index]?.label ?? "";
    messagesRef.current.push({ key: `user-sent-${sentIndexRef.current++}`, text: label, from: "user" });
    setMessages(messagesRef.current.slice());
    return true;
  };

  const answerOther = (text: string): boolean => {
    if (!canSend()) return false;
    dbg("answerOther: %d chars", text.length);
    queueRef.current!.enqueue(
      create(ClientMessageSchema, { intent: { case: "answerOther", value: { text } } }),
    );
    setSendError(null);
    messagesRef.current.push({ key: `user-sent-${sentIndexRef.current++}`, text, from: "user" });
    setMessages(messagesRef.current.slice());
    return true;
  };

  const answerMultiSelect = (indices: number[], other?: string): boolean => {
    if (!canSend()) return false;
    dbg("answerMultiSelect: indices=%o other=%o", indices, other);
    queueRef.current!.enqueue(
      create(ClientMessageSchema, {
        intent: { case: "answerMultiSelect", value: { indices, other: other ?? "" } },
      }),
    );
    setSendError(null);
    return true;
  };

  return {
    messages,
    sendPrompt,
    pendingQuestion,
    answerSelect,
    answerOther,
    answerMultiSelect,
    streamError,
    sendError,
    workflowError,
  };
}

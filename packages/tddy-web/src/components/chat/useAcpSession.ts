import { useEffect, useMemo, useRef, useState } from "react";
import { create } from "@bufbuild/protobuf";
import { createClient, type Client } from "@connectrpc/connect";
import type { Room } from "livekit-client";
import { AsyncQueue } from "tddy-livekit-web";
import {
  useLiveKitTransportFactory,
  useLiveKitTransportFactoryIsOverridden,
} from "../../rpc/transportProvider";
import {
  AcpClientMessageSchema,
  AcpService,
  type AcpClientMessage,
  type ModelSessionTarget,
  type ToolCallUpdate,
} from "../../gen/tddy/acp/v1/acp_pb";
import { tddyDebug } from "../../lib/debugMask";
import { toolEntryText, toolStatusOf } from "./toolCallPresentation";
import type {
  ChatMessage,
  ElicitationPoint,
  PendingQuestion,
  UseAgentChatResult,
} from "./useAgentChat";

/** ACP-transport diagnostics, mirroring `useAgentChat`'s presenter-stream trace. Enable in DevTools:
 *  `localStorage.debug = 'tddy:acp:*'`. */
const dbg = tddyDebug("tddy:acp:session");

/**
 * `useAcpSession` — the ACP-over-LiveKit counterpart of {@link useAgentChat}. It owns the
 * `AcpService.Session` bidirectional RPC (the protobuf mirror of ACP) carried on the *same* LiveKit
 * session connection as `TddyRemote`, and exposes the identical {@link UseAgentChatResult} surface so
 * `AgentChat` can mount either transport interchangeably (see the `acp` prop).
 *
 * Flow: open the stream by eagerly enqueueing `initialize` + `new_session` (this drives the server's
 * `Session` handler → Presenter view), then stream the agent's `session/update`s. A `PromptRequest`
 * maps a chat message onto the running session; `AgentMessageChunk`s render as one merged agent
 * bubble using the same reconciliation as `useAgentChat` / tddy-core's `AgentOutputActivityLogMerge`.
 *
 * Resume: when `resumeSessionId` is a non-empty id (e.g. a reloaded view already knows its
 * `SessionEntry.sessionId`), the stream opens with `session/load` (`LoadSessionRequest`) instead of
 * `new_session`, so the agent replays the recorded turns (`user_message_chunk` + `agent_message_chunk`)
 * before continuing. Without it, the default `new_session` path starts a fresh session.
 */
export function useAcpSession(
  room: Room | null,
  serverIdentity: string,
  resumeSessionId?: string,
): UseAgentChatResult {
  const liveKitFactory = useLiveKitTransportFactory();
  const factoryIsOverridden = useLiveKitTransportFactoryIsOverridden();
  const canBuildClient = room !== null || factoryIsOverridden;

  const client = useMemo(() => {
    if (!canBuildClient) {
      dbg("no client: canBuildClient=false — stream will NOT open");
      return null;
    }
    dbg("building AcpService client: room=%o serverIdentity=%o", room !== null, serverIdentity);
    const transport = liveKitFactory(room as Room, serverIdentity);
    return createClient(AcpService, transport);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [liveKitFactory, canBuildClient, room, serverIdentity]);

  return useAcpSessionOverClient(client, resumeSessionId, { room, identity: serverIdentity });
}

/**
 * What a chat with a row of the daemon's model registry has to name in its handshake.
 *
 * Both halves are the session's own property rather than the stream's: one stream may open a
 * session for a different assistant next time, and each assistant runs its tools somewhere else.
 */
export interface RegistryChatSession {
  /** Which registry row this session speaks as — a model, or an assistant. */
  readonly target: ModelSessionTarget;
  /**
   * The workspace an assistant's tools run in, on the daemon serving the chat. `""` for a chat that
   * runs no tools; the daemon refuses an empty workspace for a tool-bearing assistant rather than
   * choosing one on the operator's behalf.
   */
  readonly cwd: string;
}

/**
 * The ACP session itself, over a client the caller already holds.
 *
 * {@link useAcpSession} builds that client from a room plus a participant identity, which is what a
 * session presenter needs. A caller that addresses an agent through an existing daemon-level client
 * (`useDaemonClientFor(AcpService, …)` — the models chat) hands it in directly instead.
 *
 * `peer`, when given, names the participant expected to serve the stream: a send is refused with a
 * message rather than queued into a stream nobody is reading when that participant has left the
 * room. Omit it when presence is not the caller's liveness signal — a daemon that answers RPC over
 * the common room is, by definition, in it.
 *
 * `registry`, when given, rides the `new_session` handshake. A session-hosted ACP stream needs none
 * — the agent *is* that session's workflow. The daemon-hosted surface serves every model and
 * assistant in one registry, so it has to be told which one this session speaks as, and where an
 * assistant's tools may run.
 */
export function useAcpSessionOverClient(
  client: Client<typeof AcpService> | null,
  resumeSessionId?: string,
  peer?: { room: Room | null; identity: string },
  registry?: RegistryChatSession,
): UseAgentChatResult {
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [elicitations, setElicitations] = useState<ElicitationPoint[]>([]);
  const elicitationsRef = useRef<ElicitationPoint[]>([]);
  const recordElicitation = (e: ElicitationPoint) => {
    elicitationsRef.current = [...elicitationsRef.current, e];
    setElicitations(elicitationsRef.current);
  };
  const [pendingQuestion, setPendingQuestion] = useState<PendingQuestion | null>(null);
  const [streamError, setStreamError] = useState<string | null>(null);
  const [sendError, setSendError] = useState<string | null>(null);
  const [workflowError, setWorkflowError] = useState<string | null>(null);
  const queueRef = useRef<AsyncQueue<AcpClientMessage> | null>(null);

  // ACP request-id counter (envelope correlation). `initialize`/`new_session` take 1/2; prompts and
  // permission replies increment from there.
  const nextIdRef = useRef<bigint>(3n);
  // Session handle returned by new_session; stamped on outbound PromptRequests. The server drives one
  // Presenter regardless, so an empty handle before the response arrives is harmless.
  const sessionIdRef = useRef<string>("");
  const hasSentRef = useRef(false);
  const sentIndexRef = useRef(0);
  const systemKeyRef = useRef(0);
  // Distinct key space for user turns replayed by the agent on resume (vs. locally-echoed prompts).
  const replayedUserKeyRef = useRef(0);
  // Where each announced tool call's bubble sits, and the title it was announced under, by
  // `tool_call_id`. The `tool_call_update` that reports how a call ended carries neither position
  // nor, usually, a title — so without this it could only become a second, contextless entry.
  // Mirrors `acpReplayProjection`'s `toolIndexById`.
  const toolEntriesRef = useRef(new Map<string, { index: number; title: string }>());

  // --- Streaming agent-output merge (identical to useAgentChat's appendAgentChunk) ---------------
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
    messagesRef.current.push({ key: `agent-line-${agentKeyRef.current++}`, text, from: "agent", at: Date.now() });
  };
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
  const appendAgentChunk = (text: string) => {
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

  /**
   * A `tool_call_update`: how a tool call the agent already announced ended, and what it produced.
   *
   * The call's own bubble becomes a tool entry carrying that status and result. Until this arrives,
   * a call that ran, one still running and one that failed all read identically — the announcement
   * alone says only that the agent asked for it.
   */
  const applyToolCallUpdate = (call: ToolCallUpdate) => {
    const id = call.toolCallId?.value ?? "";
    const announced = id ? toolEntriesRef.current.get(id) : undefined;
    const title = call.fields?.title ?? announced?.title ?? "";
    const entry: ChatMessage = {
      // An update for a call nobody announced still gets an entry: it happened, and dropping it
      // would leave a tool the agent ran invisible.
      key: announced ? messagesRef.current[announced.index].key : `system-${systemKeyRef.current++}`,
      text: toolEntryText(title, call.fields?.rawOutput),
      from: "tool",
      at: Date.now(),
      toolStatus: toolStatusOf(call.fields?.status),
      ...(id ? { toolCallId: id } : {}),
    };
    if (announced) {
      messagesRef.current[announced.index] = entry;
    } else {
      messagesRef.current.push(entry);
    }
    if (id) {
      toolEntriesRef.current.set(id, {
        index: announced?.index ?? messagesRef.current.length - 1,
        title,
      });
    }
    setMessages(messagesRef.current.slice());
  };

  useEffect(() => {
    setMessages([]);
    setElicitations([]);
    elicitationsRef.current = [];
    setPendingQuestion(null);
    setStreamError(null);
    setSendError(null);
    setWorkflowError(null);
    hasSentRef.current = false;
    sentIndexRef.current = 0;
    systemKeyRef.current = 0;
    replayedUserKeyRef.current = 0;
    toolEntriesRef.current = new Map();
    nextIdRef.current = 3n;
    // Resuming a known session: stamp outbound prompts with its id up front (before any server
    // response), so a prompt sent before the load round-trips still targets the right session.
    sessionIdRef.current = resumeSessionId ?? "";
    messagesRef.current = [];
    agentBufferRef.current = "";
    agentPartialActiveRef.current = false;
    agentKeyRef.current = 0;
    if (!client) {
      queueRef.current = null;
      return;
    }

    const queue = new AsyncQueue<AcpClientMessage>();
    queueRef.current = queue;
    let cancelled = false;

    // Open the stream by handshaking: initialize (id 1) then either session/load (resume) or
    // new_session (id 2). The first enqueued frame makes the LiveKit transport publish the
    // stream-open, running the server's Session handler.
    queue.enqueue(create(AcpClientMessageSchema, { id: 1n, msg: { case: "initialize", value: {} } }));
    queue.enqueue(
      resumeSessionId
        ? create(AcpClientMessageSchema, {
            id: 2n,
            msg: { case: "loadSession", value: { sessionId: { value: resumeSessionId }, cwd: "" } },
          })
        : create(AcpClientMessageSchema, {
            id: 2n,
            msg: {
              case: "newSession",
              value: { cwd: registry?.cwd ?? "", modelTarget: registry?.target },
            },
          }),
    );

    dbg("opening AcpService.Session (peer identity=%o)", peer?.identity ?? "");
    (async () => {
      try {
        for await (const m of client.session(queue)) {
          if (cancelled) break;
          const msg = m.msg;
          switch (msg.case) {
            case "newSession":
              sessionIdRef.current = msg.value.sessionId?.value ?? "";
              dbg("recv new_session id=%o", sessionIdRef.current);
              break;
            case "sessionUpdate": {
              const update = msg.value.update?.update;
              if (update?.case === "agentMessageChunk") {
                const block = update.value.content?.block;
                if (block?.case === "text") {
                  appendAgentChunk(block.value.text);
                }
              } else if (update?.case === "agentThoughtChunk") {
                // tddy convention: the thought channel carries the workflow goal → "goal" bubble.
                const block = update.value.content?.block;
                if (block?.case === "text") {
                  messagesRef.current.push({
                    key: `system-${systemKeyRef.current++}`,
                    text: block.value.text,
                    from: "goal",
                    at: Date.now(),
                  });
                  setMessages(messagesRef.current.slice());
                }
              } else if (update?.case === "toolCall") {
                // Tool calls (real + one-shot activity/system log lines) → "activity" bubble. Its
                // position is remembered so the `tool_call_update` reporting the call's result
                // refines this same bubble rather than appending a second one.
                const toolCallId = update.value.toolCallId?.value ?? "";
                if (toolCallId) {
                  toolEntriesRef.current.set(toolCallId, {
                    index: messagesRef.current.length,
                    title: update.value.title,
                  });
                }
                messagesRef.current.push({
                  key: `system-${systemKeyRef.current++}`,
                  text: update.value.title,
                  from: "activity",
                  at: Date.now(),
                });
                setMessages(messagesRef.current.slice());
              } else if (update?.case === "toolCallUpdate") {
                applyToolCallUpdate(update.value);
              } else if (update?.case === "userMessageChunk") {
                // A user turn replayed by the agent on resume. `sendPrompt` already echoes a
                // locally-sent prompt as a "user" bubble, so on the LIVE path the agent's
                // `user_message_chunk` echo of that same prompt is a duplicate and must be dropped.
                // The replay, by contrast, arrives BEFORE the operator sends any prompt in this
                // session (`hasSentRef` is still false), so gate rendering on that: render only the
                // replayed historical turns, skip the live echo.
                if (!hasSentRef.current) {
                  const block = update.value.content?.block;
                  if (block?.case === "text") {
                    messagesRef.current.push({
                      key: `user-replayed-${replayedUserKeyRef.current++}`,
                      text: block.value.text,
                      from: "user",
                      at: Date.now(),
                    });
                    setMessages(messagesRef.current.slice());
                  }
                }
              }
              // `plan` carries no additional bubble; ignored on purpose.
              break;
            }
            case "requestPermission": {
              // tddy conventions: the `:multi` tool-call-id suffix marks a multi-select
              // clarification; the question text + header ride the tool-call fields (title =
              // question, raw_input = header); the "other" option is the free-text affordance.
              const toolCall = msg.value.toolCall;
              const isMulti = toolCall?.toolCallId?.value?.endsWith(":multi") ?? false;
              const options = msg.value.options
                .filter((o) => o.optionId?.value !== "other")
                .map((o) => ({ label: o.name, description: "" }));
              const allowOther = msg.value.options.some((o) => o.optionId?.value === "other");
              setPendingQuestion({
                kind: isMulti ? "multiSelect" : "select",
                header: toolCall?.fields?.rawInput ?? "",
                question: toolCall?.fields?.title ?? "",
                options,
                allowOther,
              });
              recordElicitation({
                at: Date.now(),
                kind: isMulti ? "multiSelect" : "select",
                header: toolCall?.fields?.rawInput ?? "",
                question: toolCall?.fields?.title ?? "",
                options: options.map((o) => o.label),
                allowOther,
              });
              break;
            }
            case "prompt":
              // Terminal PromptResponse: the turn ended.
              dbg("recv prompt response stopReason=%o", msg.value.stopReason);
              setPendingQuestion(null);
              break;
            case "error":
              dbg("recv error: %o", msg.value.message);
              setWorkflowError(msg.value.message || "Agent error.");
              break;
            default:
              break;
          }
        }
        dbg("stream closed (cancelled=%o)", cancelled);
      } catch (err) {
        if (!cancelled) {
          console.debug("[useAcpSession] stream error", err);
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
  }, [client, resumeSessionId, registry]);

  const canSend = (): boolean => {
    if (!queueRef.current) {
      setSendError("Message not sent — no connection to the presenter yet.");
      return false;
    }
    if (peer?.room && !peer.room.remoteParticipants.has(peer.identity)) {
      setSendError("Message not sent — the presenter is not connected.");
      return false;
    }
    return true;
  };

  const sendPrompt = (text: string): boolean => {
    if (!canSend()) return false;
    hasSentRef.current = true;
    queueRef.current!.enqueue(
      create(AcpClientMessageSchema, {
        id: nextIdRef.current++,
        msg: {
          case: "prompt",
          value: {
            sessionId: { value: sessionIdRef.current },
            prompt: [{ block: { case: "text", value: { text } } }],
          },
        },
      }),
    );
    setSendError(null);
    messagesRef.current.push({ key: `user-sent-${sentIndexRef.current++}`, text, from: "user", at: Date.now() });
    setMessages(messagesRef.current.slice());
    return true;
  };

  /** Reply to the agent's `request_permission` by selecting an option whose `option_id` encodes the
   *  answer (tddy conventions decoded by `convert_acp::permission_response_to_intent`). */
  const sendPermissionReply = (optionId: string): boolean => {
    if (!canSend()) return false;
    queueRef.current!.enqueue(
      create(AcpClientMessageSchema, {
        id: nextIdRef.current++,
        msg: {
          case: "requestPermission",
          value: {
            outcome: {
              outcome: { case: "selected", value: { optionId: { value: optionId } } },
            },
          },
        },
      }),
    );
    setSendError(null);
    setPendingQuestion(null);
    return true;
  };

  /** `option-{index}` → `AnswerSelect(index)`. */
  const answerSelect = (index: number): boolean => {
    const label = pendingQuestion?.options[index]?.label ?? "";
    if (!sendPermissionReply(`option-${index}`)) return false;
    messagesRef.current.push({ key: `user-sent-${sentIndexRef.current++}`, text: label, from: "user", at: Date.now() });
    setMessages(messagesRef.current.slice());
    return true;
  };

  /** `other:{text}` → `AnswerOther(text)` (the custom answer rides the option id, no extra prompt). */
  const answerOther = (text: string): boolean => {
    if (!sendPermissionReply(`other:${text}`)) return false;
    messagesRef.current.push({ key: `user-sent-${sentIndexRef.current++}`, text, from: "user", at: Date.now() });
    setMessages(messagesRef.current.slice());
    return true;
  };

  /** `multi:{i,j}[;other={text}]` → `AnswerMultiSelect([i,j], other?)`. */
  const answerMultiSelect = (indices: number[], other?: string): boolean => {
    const trimmed = other?.trim();
    const optionId =
      `multi:${indices.join(",")}` + (trimmed ? `;other=${trimmed}` : "");
    return sendPermissionReply(optionId);
  };

  return {
    messages,
    elicitations,
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

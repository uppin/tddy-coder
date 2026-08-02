import type { Room } from "livekit-client";
import type { CommonRoomStatus } from "../../hooks/useCommonRoom";
import { AgentTranscriptView } from "./AgentTranscriptView";
import { LiveAgentChatView } from "./LiveAgentChatView";
import { useAgentChat, type ChatMessage, type UseAgentChatResult } from "./useAgentChat";
import { useAcpSession } from "./useAcpSession";

export { TRANSCRIPT_ROOT_STYLE } from "./AgentTranscriptView";

export interface AgentChatProps {
  room: Room | null;
  livekitServerIdentity?: string;
  /** Live chat only: placeholder shown in the free-text input. Defaults to "Message the agent…".
   *  A read-only transcript has no input, so it ignores this. */
  placeholder?: string;
  /** Live chat only: status of the presenter LiveKit room connection the caller has established.
   *  A read-only transcript replays from disk over a plain client and has no room to report on, so
   *  it neither reads this nor renders a connecting state. */
  roomStatus?: CommonRoomStatus;
  /** Live chat only: error from the room connection attempt — meaningful when
   *  `roomStatus === "error"`. A read-only transcript renders no error banner: a replay that fails
   *  keeps whatever frames arrived rather than fabricating a message about them
   *  (`docs/ft/web/agent-activity-pane.md` § Tail-first, auto-scrolling transcript). */
  roomError?: string | null;
  /** Drive the session over the ACP protobuf mirror (`AcpService.Session`) instead of the default
   *  `TddyRemote.Stream`. Both ride the same LiveKit session connection and render identically. */
  acp?: boolean;
  /** Resume an existing session by id instead of starting a new one: the ACP stream opens with
   *  `session/load` so the agent replays the prior conversation (used after a browser reload).
   *  Only meaningful with `acp`. */
  resumeSessionId?: string;
  /** Render as a read-only transcript: no message input, Send button, or clarification composer,
   *  and each entry gains a right-aligned "+Ns" elapsed badge (plus a status marker on tool calls).
   *  Used by the Agent Activity overlay to replay a session's ACP conversation. */
  readOnly?: boolean;
  /** Read-only transcript only: invoked when a `from: "tool"` entry is clicked, so the host can open
   *  its detail dialog. Non-tool entries stay inert. Unset ⇒ no entry is interactive. */
  onToolClick?: (message: ChatMessage) => void;
  /** Read-only transcript only: invoked when the reader reaches the start of the loaded range, so
   *  the host can page in the history before it. Unset ⇒ the range never grows backwards. */
  onLoadOlder?: () => void;
  /** Read-only transcript only: whether any history exists before the loaded range. False closes the
   *  range — no scroll asks for another page. */
  hasOlder?: boolean;
  /** Read-only transcript only: whether a page of older history is in flight. */
  loadingOlder?: boolean;
}

/**
 * Recipe-agnostic chat window over a session's remote agent. By default it speaks the Presenter's
 * `TddyRemote.Stream`; with `acp`, it speaks the ACP protobuf mirror (`AcpService.Session`) over the
 * same LiveKit session connection. Hook selection lives in the two backed wrappers below so neither
 * hook is called conditionally; both hand their result to `AgentChatView`, which picks the surface.
 */
export function AgentChat(props: AgentChatProps) {
  return props.acp ? <AcpBackedChat {...props} /> : <RemoteBackedChat {...props} />;
}

function RemoteBackedChat(props: AgentChatProps) {
  const chat = useAgentChat(props.room, props.livekitServerIdentity || "server");
  return <AgentChatView {...props} chat={chat} />;
}

function AcpBackedChat(props: AgentChatProps) {
  const chat = useAcpSession(props.room, props.livekitServerIdentity || "server", props.resumeSessionId);
  return <AgentChatView {...props} chat={chat} />;
}

/**
 * The presentation shared by every backing hook, over two unrelated surfaces: an interactive chat
 * with a composer ({@link LiveAgentChatView}) and a recorded transcript that follows its own tail
 * ({@link AgentTranscriptView}). `readOnly` picks between them — they overlap only in how a single
 * entry is styled, so each owns its own layout, its own affordances and its own hooks.
 */
export function AgentChatView({
  placeholder,
  roomStatus,
  roomError,
  readOnly = false,
  onToolClick,
  onLoadOlder,
  hasOlder,
  loadingOlder,
  chat,
}: AgentChatProps & { chat: UseAgentChatResult }) {
  if (readOnly) {
    return (
      <AgentTranscriptView
        messages={chat.messages}
        onToolClick={onToolClick}
        onLoadOlder={onLoadOlder}
        hasOlder={hasOlder}
        loadingOlder={loadingOlder}
      />
    );
  }
  return (
    <LiveAgentChatView
      placeholder={placeholder}
      roomStatus={roomStatus}
      roomError={roomError}
      chat={chat}
    />
  );
}

/**
 * `SessionRuntime`'s contract with its callers, and the client aliases it and its nested child
 * runtime share. Lifted out of `SessionRuntime.tsx` unchanged (mirrors `createSessionPaneProps.ts`).
 */

import type { Client } from "@connectrpc/connect";
import type { SessionEntry } from "../../gen/session_pb";
import type { SessionAgentService } from "../../gen/session_agents_pb";
import type { TerminalSessionService } from "../../gen/terminal_session_pb";
import type { HostConnection } from "../../rpc/connections/types";
import type { ToolShortcutDef } from "../../lib/toolShortcuts";
import type { AgentConversation } from "./agentConversationTabs";
import type { ByteDelta, SessionRuntimeState } from "./sessionRuntimeRegistry";

export type SessionAgentClient = Client<typeof SessionAgentService>;
export type TerminalClient = Client<typeof TerminalSessionService>;

export interface SessionRuntimeProps {
  /** This runtime's attached-session state (connection params + status). */
  runtime: SessionRuntimeState;
  /** True when this runtime is the focused (CSS-visible) one. Drives the control toast + mobile
   *  shortcut overlay; backgrounded runtimes stay mounted but `display:none`. */
  focused: boolean;
  sessionToken: string;
  /** The signed-in user's login, which an agent-conversation tab needs in order to cancel when it
   *  is closed. `null` (the default) when none is signed in, and the closing cancel is refused. */
  callerLogin?: string | null;
  /** Owning daemon `session_agents.SessionAgentService` client — the agent-conversation panes hold
   *  their conversation over it, and a spawned child runtime inherits it. Pass `null`/`undefined`
   *  until the daemon is reachable. */
  client?: SessionAgentClient | null;
  /** Owning daemon `TerminalSessionService` client — the auto-claim-on-attach target, and the
   *  fallback wire for host-served terminal I/O. Pass `null`/`undefined` until the daemon is
   *  reachable. */
  terminalClient?: TerminalClient | null;
  /** The connection to the session's owning daemon — a spawned child conversation attaches its own
   *  session over it. `null` until a host is reachable, which is when no child can be attached. */
  host?: HostConnection | null;
  /** Shortcut presets — shown as the mobile shortcut overlay on the focused runtime only. */
  mobileShortcuts?: ToolShortcutDef[];
  /** Register this session's Agent-terminal text-insert (for the inspector Files-tab click/tap
   *  route). Fired once the terminal mounts. */
  onSessionRegisterInsert?: (sessionId: string, insertInput: (text: string) => void) => void;
  /** Evict this runtime's terminal (e.g. remote session ended). */
  onSessionDisconnect?: (sessionId: string) => void;
  /** Account this session's terminal I/O bytes (see `GhosttyTerminalSession.onBytes`) so the
   *  screen can fold them into the session's inspector counters. */
  onSessionBytes?: (sessionId: string, delta: ByteDelta) => void;
  /** The drawer's full session list — used to discover this session's spawned child conversations
   *  (`orchestratorSessionId === this session`) and render them as tabs. */
  sessions?: ReadonlyArray<SessionEntry>;
  /** Open conversations with agents attached to this session, one tab and one pane each. Held by the
   *  screen rather than here so switching sessions (which backgrounds this runtime) cannot end one. */
  agentConversations?: readonly AgentConversation[];
  /** The focused conversation's id, or `null` when a terminal or child tab holds the pane. */
  activeAgentConversationId?: string | null;
  /** Focus a conversation, or `null` to hand the pane back to the terminal/child tabs. */
  onSelectAgentConversation?: (conversationId: string | null) => void;
  /** Close a conversation's tab — the owner drops it, which cancels it. */
  onCloseAgentConversation?: (conversationId: string) => void;
}

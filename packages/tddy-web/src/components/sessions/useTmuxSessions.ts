import type { TerminalClient } from "./sessionRuntimeProps";
import type { TmuxSessionInfo } from "../../gen/terminal_session_pb";

/**
 * The tmux sessions on a session's host that a "tmux" tab can attach to, fetched through the
 * owning daemon's `TerminalSessionService.ListTmuxSessions` each time the "+" menu opens — tmux
 * sessions come and go outside tddy, so a list cached from an earlier opening would offer stale
 * ones.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-11-02-tmux-attach-tab.md (`#session-tabs 3/7`)
 */
export interface TmuxMenuState {
  /** The sessions from the last listing, in the host's order. */
  readonly sessions: readonly TmuxSessionInfo[];
  /** Why the last listing failed (e.g. tmux is not installed), ready to render; `null` otherwise. */
  readonly error: string | null;
  /** List again — called when the "+" menu opens. */
  readonly refresh: () => void;
}

export interface UseTmuxSessionsParams {
  sessionId: string;
  sessionToken: string;
  /** The owning daemon's terminal client; no listing is made without one. */
  client?: TerminalClient | null;
}

export function useTmuxSessions(_params: UseTmuxSessionsParams): TmuxMenuState {
  // TODO(tmux-attach): implement
  return { sessions: [], error: null, refresh: () => undefined };
}

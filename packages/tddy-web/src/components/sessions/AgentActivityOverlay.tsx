import { useEffect, useState } from "react";
import type { Client } from "@connectrpc/connect";
import { ConnectionService } from "../../gen/connection_pb";
import { useHttpClient } from "../../rpc/transportProvider";
import { AgentChatView } from "../chat/AgentChat";
import { useAcpReplay } from "../chat/useAcpReplay";
import { Button } from "../ui/button";

interface AgentActivityOverlayProps {
  sessionId: string;
  sessionToken: string;
  /** Session type is carried for parity with the rest of the session chrome; the pane itself is
   *  session-type-agnostic (it renders identically for tool, sandbox, claude-cli, … sessions). */
  sessionType?: string;
  /** Explicit client override — session-scoped routing where available. Falls back to the shared
   *  HTTP client from the transport context. */
  client?: Client<typeof ConnectionService>;
}

/**
 * Per-session, top-bar overlay that replays the agent's own ACP conversation as a read-only
 * transcript (streamed via `ConnectionService.StreamAcpReplay`). The icon appears only once the
 * session has produced at least one transcript entry; an unread badge flags activity that arrived
 * since the overlay was last opened. The body is the shared `AgentChatView` in read-only mode:
 * agent text interleaved with enriched tool calls, each stamped with a "+Ns" elapsed badge — no
 * composer (this is a replay, not an interactive chat).
 *
 * PRD: docs/ft/web/agent-activity-pane.md § Read-only ACP transcript.
 */
export function AgentActivityOverlay({
  sessionId,
  sessionToken,
  client,
}: AgentActivityOverlayProps) {
  // `useHttpClient` is called unconditionally (hook rules); the explicit prop wins when present.
  const httpClient = useHttpClient(ConnectionService);
  const resolvedClient = client ?? httpClient;

  const chat = useAcpReplay({ sessionId, sessionToken, client: resolvedClient });
  const { hasActivity, unreadCount, markSeen, messages } = chat;

  const [open, setOpen] = useState(false);

  // While the overlay is open, freshly-arriving entries are considered seen the moment they land.
  useEffect(() => {
    if (open) markSeen();
    // markSeen closes over the current messages; re-running when messages change keeps the badge
    // clear while the operator is looking at the transcript.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, messages]);

  if (!hasActivity) {
    return null;
  }

  return (
    <>
      <div className="relative inline-flex">
        <Button
          data-testid="agent-activity-button"
          variant="ghost"
          size="icon-sm"
          title="Agent activity"
          onClick={() => {
            setOpen((prev) => {
              if (!prev) markSeen();
              return !prev;
            });
          }}
        >
          <svg
            xmlns="http://www.w3.org/2000/svg"
            width="16"
            height="16"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth="2"
            strokeLinecap="round"
            strokeLinejoin="round"
          >
            <path d="M22 12h-4l-3 9L9 3l-3 9H2" />
          </svg>
        </Button>
        {unreadCount > 0 && (
          <span
            data-testid="agent-activity-unread-badge"
            className="absolute -right-0.5 -top-0.5 flex min-h-4 min-w-4 items-center justify-center rounded-full bg-primary px-1 text-[10px] font-semibold leading-none text-primary-foreground"
          >
            {unreadCount}
          </span>
        )}
      </div>

      {open && (
        <div
          data-testid="agent-activity-overlay"
          className="absolute top-0 right-0 z-10 flex h-full w-full flex-col border-l border-border bg-background md:w-[360px]"
        >
          <div className="flex flex-shrink-0 items-center justify-between border-b border-border px-3 py-2">
            <span className="text-xs font-semibold uppercase tracking-wide text-muted-foreground">
              Agent activity
            </span>
            <Button
              data-testid="agent-activity-overlay-close"
              variant="ghost"
              size="icon-sm"
              className="h-6 w-6 p-0"
              onClick={() => setOpen(false)}
              title="Close"
            >
              <svg
                xmlns="http://www.w3.org/2000/svg"
                width="14"
                height="14"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                strokeWidth="2"
                strokeLinecap="round"
                strokeLinejoin="round"
              >
                <line x1="18" y1="6" x2="6" y2="18" />
                <line x1="6" y1="6" x2="18" y2="18" />
              </svg>
            </Button>
          </div>

          <AgentChatView room={null} readOnly chat={chat} />
        </div>
      )}
    </>
  );
}

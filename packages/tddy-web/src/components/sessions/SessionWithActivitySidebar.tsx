import React, { useEffect, useState, type CSSProperties } from "react";
import type { Client } from "@connectrpc/connect";
import { ActivityService } from "../../gen/activity_pb";
import type { SessionEntry } from "../../gen/session_pb";
import { useIsNarrowViewport } from "../../hooks/useIsMobile";
import { useHttpClient } from "../../rpc/transportProvider";
import { useAcpReplay } from "../chat/useAcpReplay";
import type { ChatMessage } from "../chat/useAgentChat";
import { Button } from "../ui/button";
import { AgentActivityDetailDialog } from "./AgentActivityDetailDialog";
import { SessionActivitySidebar } from "./SessionActivitySidebar";
import { SubagentConversationDialog } from "./SubagentConversationDialog";
import { defaultActivitySidebarOpen } from "./activitySidebarState";
import { rosterHalfOf } from "./sessionRosterHalf";

/** Inline layout (PRD § Inline layout contract): the docked column and the content beside it are
 *  measured against each other, so their flex structure is not left to a stylesheet. */
const ROOT_STYLE: CSSProperties = {
  display: "flex",
  flexDirection: "column",
  flex: 1,
  minHeight: 0,
  minWidth: 0,
};
const ROW_STYLE: CSSProperties = {
  display: "flex",
  position: "relative",
  flex: 1,
  minHeight: 0,
};
const COLUMN_STYLE: CSSProperties = {
  display: "flex",
  flexDirection: "column",
  flex: 1,
  minWidth: 0,
  minHeight: 0,
};

/**
 * The session view with the Agent Activity transcript docked as a **right sidebar**
 * (`#project-home` 3/7), replacing the top-bar popover.
 *
 * - `toolbar` is where the toggle (with its unread badge) is rendered, in the main pane's header.
 *   The toggle appears once the session has transcript entries, as the popover's icon did.
 * - On selecting a session the sidebar's open state is `defaultActivitySidebarOpen(session)`; a
 *   toggle applies to that session until another one is selected.
 * - `suppressed` (the main pane already shows the Activities view) renders neither toggle nor
 *   sidebar: one transcript per pane.
 * - At mobile width the sidebar is a full-width overlay instead of a docked column.
 *
 * `children` always sit in the same slot of the tree, so opening or closing the sidebar never
 * remounts them (an open agent conversation inside them would otherwise cancel itself).
 *
 * Feature: docs/ft/web/agent-activity-pane.md § Right sidebar.
 */
export function SessionWithActivitySidebar(props: {
  session: SessionEntry;
  sessionToken: string;
  client?: Client<typeof ActivityService>;
  suppressed: boolean;
  isMobile?: boolean;
  toolbar: (toggle: React.ReactNode) => React.ReactNode;
  children: React.ReactNode;
}): React.ReactElement {
  const { session, sessionToken, suppressed, toolbar, children } = props;
  const sessionId = session.sessionId;

  // `useHttpClient` is called unconditionally (hook rules); the explicit prop wins when present.
  const httpClient = useHttpClient(ActivityService);
  const client = props.client ?? httpClient;
  // Width decides the layout here (a docked column needs room), not the input device.
  const narrow = useIsNarrowViewport();
  const mobile = props.isMobile ?? narrow;

  const chat = useAcpReplay({ sessionId, sessionToken, client });
  const { hasActivity, unreadCount, markSeen, loadSnapshot } = chat;

  // A toggle is kept against the session it was made on, and dropped when another one is selected.
  const [override, setOverride] = useState<{ sessionId: string; open: boolean } | null>(null);
  useEffect(() => {
    setOverride(null);
  }, [sessionId]);
  const open =
    override !== null && override.sessionId === sessionId
      ? override.open
      : defaultActivitySidebarOpen(session);
  const shown = open && !suppressed && hasActivity;

  const [detail, setDetail] = useState<ChatMessage | null>(null);
  // The subagent conversation whose bubble was clicked, opened over the session. Its title is the bubble's.
  const [conversation, setConversation] = useState<{ conversationId: string; title: string } | null>(
    null,
  );
  useEffect(() => {
    setDetail(null);
    setConversation(null);
  }, [sessionId]);

  // Showing the transcript pulls its lazy, once-per-session snapshot and marks what arrived seen.
  useEffect(() => {
    if (!shown) return;
    loadSnapshot();
    if (unreadCount > 0) markSeen();
  }, [shown, unreadCount, loadSnapshot, markSeen]);

  // The Activities view already shows this transcript, so nothing counted while it is up is unread.
  useEffect(() => {
    if (suppressed && unreadCount > 0) markSeen();
  }, [suppressed, unreadCount, markSeen]);

  const toggle =
    !suppressed && hasActivity ? (
      <div className="relative inline-flex">
        <Button
          data-testid="agent-activity-button"
          variant="ghost"
          size="icon-sm"
          title="Agent activity"
          aria-expanded={open}
          onClick={() => setOverride({ sessionId, open: !open })}
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
    ) : null;

  return (
    <div style={ROOT_STYLE}>
      {toolbar(toggle)}
      <div style={ROW_STYLE}>
        <div style={COLUMN_STYLE}>{children}</div>
        {shown && (
          <SessionActivitySidebar
            sessionId={sessionId}
            chat={chat}
            mobile={mobile}
            onClose={() => setOverride({ sessionId, open: false })}
            onToolClick={setDetail}
            onSubagentClick={(message) => {
              if (message.conversationId) {
                setConversation({ conversationId: message.conversationId, title: message.text });
              }
            }}
          />
        )}
      </div>
      {detail && (
        <AgentActivityDetailDialog
          message={detail}
          sessionId={sessionId}
          sessionToken={sessionToken}
          client={client}
          onClose={() => setDetail(null)}
        />
      )}
      {conversation && (
        <SubagentConversationDialog
          sessionId={sessionId}
          conversationId={conversation.conversationId}
          title={conversation.title}
          sessionToken={sessionToken}
          rosterHalf={rosterHalfOf(session)}
          client={client}
          onClose={() => setConversation(null)}
        />
      )}
    </div>
  );
}

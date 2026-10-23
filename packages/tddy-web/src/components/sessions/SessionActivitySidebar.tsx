import type { CSSProperties } from "react";
import { AgentChatView } from "../chat/AgentChat";
import type { UseAcpReplayResult } from "../chat/useAcpReplay";
import type { ChatMessage } from "../chat/useAgentChat";
import { Button } from "../ui/button";

/** Inline layout, not classes: the transcript inside only scrolls if every element above it
 *  bounds its height (PRD § Inline layout contract), and the dock's width is measured. */
const PANEL_STYLE: CSSProperties = {
  display: "flex",
  flexDirection: "column",
  height: "100%",
  minHeight: 0,
};
const DOCKED_STYLE: CSSProperties = {
  ...PANEL_STYLE,
  position: "relative",
  width: 360,
  flexShrink: 0,
};
const OVERLAY_STYLE: CSSProperties = {
  ...PANEL_STYLE,
  position: "absolute",
  inset: 0,
  zIndex: 10,
};

export interface SessionActivitySidebarProps {
  sessionId: string;
  /** The session's transcript, from the host's `useAcpReplay`. */
  chat: UseAcpReplayResult;
  /** Full-width overlay at mobile width; a docked column otherwise. */
  mobile: boolean;
  onClose: () => void;
  onToolClick: (message: ChatMessage) => void;
}

/**
 * The Agent Activity transcript as the session view's right sidebar (`#project-home` 3/7): the
 * shared read-only `AgentChatView` under a header with a close control. Its open state and its
 * toggle belong to the host (`SessionWithActivitySidebar`).
 */
export function SessionActivitySidebar({
  sessionId,
  chat,
  mobile,
  onClose,
  onToolClick,
}: SessionActivitySidebarProps) {
  const { atOldest, loadingOlder, loadOlder } = chat;
  return (
    <section
      data-testid="agent-activity-sidebar"
      className="border-l border-border bg-background"
      style={mobile ? OVERLAY_STYLE : DOCKED_STYLE}
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
          onClick={onClose}
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
      {/* Keyed by session: the sidebar is not remounted on a session switch, so the transcript
          would otherwise carry the previous session's scroll position into the next one. */}
      <AgentChatView
        key={sessionId}
        room={null}
        readOnly
        chat={chat}
        onToolClick={onToolClick}
        onLoadOlder={loadOlder}
        hasOlder={!atOldest}
        loadingOlder={loadingOlder}
      />
    </section>
  );
}

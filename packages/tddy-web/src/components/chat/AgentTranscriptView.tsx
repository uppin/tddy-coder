import React from "react";
import { Button } from "../ui/button";
import { bubbleClass, elapsedBadge, toolStatusClass } from "./chatEntryPresentation";
import { useTailFollow } from "./useTailFollow";
import type { ChatMessage } from "./useAgentChat";

/**
 * Inline duplicates of the read-only transcript's Tailwind layout classes.
 *
 * The scroll container's ability to overflow is not decoration — it is the precondition for opening
 * at the newest entry, following live activity and paging backwards. Declaring it inline as well as
 * in the class list means the behaviour does not depend on a stylesheet having been loaded, which is
 * exactly the difference between a transcript that scrolls and one that silently cannot. Every
 * element on the chain from the surface down to the scroll container carries its own declaration;
 * a bounded height that stops one level above the container bounds nothing.
 *
 * PRD: docs/ft/web/agent-activity-pane.md § Tail-first — the scroll container must declare its own layout.
 */
export const TRANSCRIPT_ROOT_STYLE: React.CSSProperties = {
  display: "flex",
  flexDirection: "column",
  flex: "1 1 0%",
  minHeight: 0,
  overflow: "hidden",
};

/** The scroll container's own half of {@link TRANSCRIPT_ROOT_STYLE}. */
const TRANSCRIPT_MESSAGES_STYLE: React.CSSProperties = {
  display: "flex",
  flexDirection: "column",
  flex: "1 1 0%",
  minHeight: 0,
  overflowY: "auto",
  // Compensating for a prepended page is this component's job (see `scrollTopAfterPrepend`); the
  // browser's own scroll anchoring would apply a second, uncoordinated correction on top of it.
  overflowAnchor: "none",
};

export interface AgentTranscriptViewProps {
  messages: ChatMessage[];
  /** Invoked when a `from: "tool"` entry is clicked, so the host can open its detail dialog. Non-tool
   *  entries stay inert. Unset ⇒ no entry is interactive. */
  onToolClick?: (message: ChatMessage) => void;
  /** Invoked when the reader reaches the start of the loaded range, so the host can page in the
   *  history before it. Unset ⇒ the range never grows backwards. */
  onLoadOlder?: () => void;
  /** Whether any history exists before the loaded range. False closes the range — no scroll asks for
   *  another page. */
  hasOlder?: boolean;
  /** Whether a page of older history is in flight. */
  loadingOlder?: boolean;
}

/**
 * The read-only half of {@link AgentChatView}: a recorded ACP conversation replayed as a transcript.
 * No composer, no clarification prompts — instead it opens on the newest entry and follows live
 * activity ({@link useTailFollow}), pages backwards when the reader reaches the start of the loaded
 * range, and stamps each entry with a "+Ns" elapsed badge plus a status marker on tool calls.
 */
export function AgentTranscriptView({
  messages,
  onToolClick,
  onLoadOlder,
  hasOlder = false,
  loadingOlder = false,
}: AgentTranscriptViewProps) {
  const follow = useTailFollow({ messages, hasOlder, loadingOlder, onLoadOlder });

  return (
    <div
      data-testid="agent-chat"
      className="relative flex-1 min-h-0 flex flex-col overflow-hidden"
      style={TRANSCRIPT_ROOT_STYLE}
    >
      {/* Both the paging indicator and the jump-to-latest affordance sit OUTSIDE the scroll
          container: inside it they would be content, and content that appears and disappears around
          a prepend is content whose height the compensating scroll would have to account for. */}
      {loadingOlder && (
        <div
          data-testid="agent-chat-older-loading"
          className="flex-shrink-0 px-3 py-1 text-center text-[10px] text-muted-foreground"
        >
          Loading earlier activity…
        </div>
      )}
      <div
        data-testid="agent-chat-messages"
        className="flex-1 min-h-0 overflow-y-auto flex flex-col gap-2 p-3"
        style={TRANSCRIPT_MESSAGES_STYLE}
        ref={follow.scrollRef}
        onScroll={follow.handleScroll}
      >
        {messages.map((m, i) => (
          <div key={m.key} className="flex items-start justify-between gap-2">
            <div
              data-testid={`agent-chat-message-${i}`}
              data-message-kind={m.from}
              className={
                m.from === "tool" && onToolClick
                  ? `${bubbleClass(m.from)} cursor-pointer hover:bg-muted`
                  : bubbleClass(m.from)
              }
              onClick={m.from === "tool" && onToolClick ? () => onToolClick(m) : undefined}
            >
              {m.from === "goal" ? `Goal: ${m.text}` : m.text}
            </div>
            <div className="flex flex-shrink-0 items-center gap-1.5 pt-1">
              {m.from === "tool" && m.toolStatus && (
                <span
                  data-testid={`agent-chat-tool-status-${i}`}
                  className={toolStatusClass(m.toolStatus)}
                >
                  {m.toolStatus}
                </span>
              )}
              <span
                data-testid={`agent-chat-elapsed-${i}`}
                className="font-mono text-[10px] leading-none text-muted-foreground"
              >
                {elapsedBadge(messages, i)}
              </span>
            </div>
          </div>
        ))}
      </div>
      {follow.arrivedWhileDetached > 0 && (
        <div className="flex flex-shrink-0 justify-center py-1">
          <Button
            data-testid="agent-chat-jump-to-latest"
            variant="secondary"
            size="sm"
            className="h-6 px-2 text-xs"
            onClick={follow.jumpToLatest}
          >
            {`${follow.arrivedWhileDetached} new ↓`}
          </Button>
        </div>
      )}
      {/* The declared source of truth for the transcript's viewport: specs read scroll facts off
          this mirror rather than re-deriving them from layout, so a style change cannot quietly turn
          a scroll assertion green. */}
      <div
        data-testid="agent-chat-scroll-state"
        hidden
        data-pinned={follow.pinned ? "true" : "false"}
        data-scroll-top={follow.viewport.scrollTop}
        data-scroll-height={follow.viewport.scrollHeight}
        data-client-height={follow.viewport.clientHeight}
      />
    </div>
  );
}

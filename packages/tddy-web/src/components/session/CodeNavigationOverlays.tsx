import { useLayoutEffect, useRef, useState, type RefObject } from "react";

import type { CodeLocationRef, CodePosition } from "./codeNavigationApi";
import { renderSimpleMarkdown } from "./renderSimpleMarkdown";
import type { Hover, LocationList, Notice } from "./useCodeNavigation";

export function NavigationNotice({ notice, loadingText }: { notice: Notice; loadingText: string }) {
  const isError = notice.kind === "message" && notice.isError;
  return (
    <p
      data-testid="worktree-code-notice"
      role={isError ? "alert" : "status"}
      className={`pointer-events-auto rounded border border-border bg-background p-2 text-sm ${
        isError ? "text-destructive" : "text-muted-foreground"
      }`}
    >
      {notice.kind === "held" ? loadingText : notice.text}
    </p>
  );
}

/** Space between the identifier and the card, in CSS pixels. */
const ANCHOR_GAP_PX = 4;

type CardPosition = { left: number; top: number };

/**
 * Where the card goes inside its containing block (the preview's positioned wrapper): just below
 * the identifier at `at`, flipped above when it would run past the bottom and there is room above,
 * and shifted left when it would run past the right edge — never past the identifier's own left
 * edge. The identifier is looked up afresh each time, so a preview that re-rendered it is followed;
 * `null` when it is not shown (the card keeps where it was).
 */
function positionBeside(card: HTMLElement, at: CodePosition): CardPosition | null {
  const container = card.offsetParent;
  if (container === null) return null;
  const anchor = container.querySelector<HTMLElement>(
    `[data-identifier-at="${at.line}:${at.column}"]`,
  );
  if (anchor === null) return null;
  const box = container.getBoundingClientRect();
  const word = anchor.getBoundingClientRect();
  const rem = parseFloat(getComputedStyle(document.documentElement).fontSize);
  const width = card.offsetWidth;
  const height = card.offsetHeight;

  const below = word.bottom - box.top + ANCHOR_GAP_PX;
  const above = word.top - box.top - ANCHOR_GAP_PX - height;
  const top = below + height > box.height && above >= 0 ? above : below;
  const left = Math.max(0, Math.min(word.left - box.left, box.width - rem - width));
  return { left, top };
}

/**
 * Where the card in `cardRef` goes beside the identifier at `at`: `null` until it is first laid
 * out. Runs after every render, since the card's size follows its content (loading text, then
 * markdown).
 */
function useCardPosition(
  cardRef: RefObject<HTMLDivElement | null>,
  at: CodePosition,
): CardPosition | null {
  const [position, setPosition] = useState<CardPosition | null>(null);
  useLayoutEffect(() => {
    const card = cardRef.current;
    if (card === null) return;
    const next = positionBeside(card, at);
    if (next === null) return;
    setPosition((prev) =>
      prev !== null && prev.left === next.left && prev.top === next.top ? prev : next,
    );
  });
  return position;
}

/** The card's content: `loadingText` while the index holds the query, then the markdown, scrolling. */
function HoverCardBody({ content, loadingText }: { content: Hover["content"]; loadingText: string }) {
  if (content.kind === "loading") {
    return (
      <p data-testid="worktree-code-hover-loading" className="p-2 text-sm text-muted-foreground">
        {loadingText}
      </p>
    );
  }
  return (
    <div
      data-testid="worktree-code-hover-body"
      className="min-h-0 flex-1 overflow-auto p-2 text-sm [overflow-wrap:anywhere]"
    >
      {renderSimpleMarkdown(content.markdown)}
    </div>
  );
}

/** References / Close, kept in view below the scrolling body. */
function HoverCardActions({
  onShowReferences,
  onClose,
}: {
  onShowReferences: () => void;
  onClose: () => void;
}) {
  return (
    <div className="flex shrink-0 gap-2 border-t border-border px-2 py-1">
      <button
        type="button"
        data-testid="worktree-code-references-action"
        className="text-sm underline"
        onClick={onShowReferences}
      >
        References
      </button>
      <button
        type="button"
        data-testid="worktree-code-hover-close"
        className="text-sm underline"
        onClick={onClose}
      >
        Close
      </button>
    </div>
  );
}

/**
 * The hover card, floating beside the hovered identifier: at most `min(40rem, 100% − 2rem)` of the
 * preview wide and `20rem` tall, its rendered markdown scrolling inside while References / Close
 * stay in view. While the index holds the query it shows `loadingText` instead.
 */
export function HoverCard({
  hover,
  loadingText,
  onShowReferences,
  onClose,
}: {
  hover: Hover;
  loadingText: string;
  onShowReferences: () => void;
  onClose: () => void;
}) {
  const cardRef = useRef<HTMLDivElement | null>(null);
  const position = useCardPosition(cardRef, hover.at);

  return (
    <div
      ref={cardRef}
      data-testid="worktree-code-hover"
      className="pointer-events-auto absolute z-10 flex max-h-80 flex-col rounded border border-border bg-background shadow"
      style={{
        left: position?.left ?? 0,
        top: position?.top ?? 0,
        width: "max-content",
        maxWidth: "min(40rem, calc(100% - 2rem))",
        visibility: position === null ? "hidden" : undefined,
      }}
    >
      <HoverCardBody content={hover.content} loadingText={loadingText} />
      <HoverCardActions onShowReferences={onShowReferences} onClose={onClose} />
    </div>
  );
}

export function LocationListCard({
  list,
  onOpen,
}: {
  list: LocationList;
  onOpen: (location: CodeLocationRef) => void;
}) {
  return (
    <div
      data-testid="worktree-code-references"
      className="pointer-events-auto max-h-48 overflow-auto rounded border border-border bg-background p-2 shadow"
    >
      <p className="text-sm font-medium">{list.title}</p>
      {list.locations.length === 0 && (
        <p className="text-sm text-muted-foreground">None found</p>
      )}
      <ul>
        {list.locations.map((location) => (
          <li key={`${location.relPath}:${location.start.line}:${location.start.column}`}>
            <button
              type="button"
              data-testid={`worktree-code-reference-${location.relPath}-${location.start.line}`}
              disabled={location.outsideWorktree}
              className="text-left font-mono text-sm underline disabled:cursor-not-allowed disabled:no-underline disabled:opacity-60"
              onClick={() => onOpen(location)}
            >
              {location.relPath}:{location.start.line}
              {location.outsideWorktree ? " (outside the worktree)" : ""}
            </button>
          </li>
        ))}
      </ul>
    </div>
  );
}

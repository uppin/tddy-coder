import { useLayoutEffect, useRef, useState } from "react";

import type { CodeLocationRef } from "./codeNavigationApi";
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
 * its anchor, flipped above when it would run past the bottom and there is room above, and shifted
 * left when it would run past the right edge — never past the anchor's own left edge.
 */
function positionBeside(card: HTMLElement, anchor: HTMLElement): CardPosition | null {
  const container = card.offsetParent;
  if (container === null) return null;
  const box = container.getBoundingClientRect();
  const at = anchor.getBoundingClientRect();
  const rem = parseFloat(getComputedStyle(document.documentElement).fontSize);
  const width = card.offsetWidth;
  const height = card.offsetHeight;

  const below = at.bottom - box.top + ANCHOR_GAP_PX;
  const above = at.top - box.top - ANCHOR_GAP_PX - height;
  const top = below + height > box.height && above >= 0 ? above : below;
  const left = Math.max(0, Math.min(at.left - box.left, box.width - rem - width));
  return { left, top };
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
  const [position, setPosition] = useState<CardPosition | null>(null);

  // Runs after every render: the card's size follows its content (loading text, then markdown).
  useLayoutEffect(() => {
    const card = cardRef.current;
    if (card === null) return;
    const next = positionBeside(card, hover.anchor);
    if (next === null) return;
    setPosition((prev) =>
      prev !== null && prev.left === next.left && prev.top === next.top ? prev : next,
    );
  });

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
      {hover.content.kind === "loading" ? (
        <p data-testid="worktree-code-hover-loading" className="p-2 text-sm text-muted-foreground">
          {loadingText}
        </p>
      ) : (
        <div
          data-testid="worktree-code-hover-body"
          className="min-h-0 flex-1 overflow-auto p-2 text-sm [overflow-wrap:anywhere]"
        >
          {renderSimpleMarkdown(hover.content.markdown)}
        </div>
      )}
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

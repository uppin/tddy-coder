import React from "react";
import { Button } from "../../ui/button";
import { cn } from "../../../lib/utils";

/** Visibility of the Planned-PRs panel. Mirrors `SessionInspectorDrawer`'s `data-state` contract. */
export type PlannedPrPanelState = "closed" | "open";

/** Width of the docked panel on desktop. Also the column the chat reserves for it. */
export const PLANNED_PR_PANEL_WIDTH_PX = 360;

export interface PlannedPrPanelProps {
  state: PlannedPrPanelState;
  /** Full-screen overlay when true, docked column when false — same contract as `SessionDrawer`. */
  isMobile: boolean;
  /** Dismiss the panel from its own close control. */
  onClose: () => void;
  /** The planned-PR list, its "+ New planned PR" entry point, and the add form. */
  children: React.ReactNode;
}

/**
 * The "Planned PRs" panel: a docked column to the right of the chat on desktop, a full-screen overlay
 * on mobile. Same contract as `SessionInspectorDrawer` — always rendered, with `data-state` driving
 * visibility — so the list keeps its scroll position and the view keeps its branch poll set across a
 * close and reopen.
 *
 * Positioned against its parent, which must be `relative`. It deliberately does not cover the screen's
 * own header, so the toggle that opens it stays reachable while it is open.
 *
 * Width and visibility are inline rather than Tailwind utilities, following `SessionDrawer`: they are
 * the panel's layout contract rather than decoration, so they hold wherever the component is mounted.
 * Mobile is the `isMobile` prop rather than a `md:` media query for the same reason.
 */
export function PlannedPrPanel({ state, isMobile, onClose, children }: PlannedPrPanelProps) {
  return (
    <div
      data-testid="pr-stack-planned-pr-panel"
      data-state={state}
      className={cn(
        "flex flex-col h-full border-l border-border bg-background overflow-hidden",
        "absolute top-0 right-0 z-10",
        isMobile ? "w-full" : "w-[360px]",
      )}
      style={{
        width: isMobile ? "100%" : PLANNED_PR_PANEL_WIDTH_PX,
        ...(state === "closed" ? { display: "none" } : {}),
      }}
    >
      <div className="flex flex-shrink-0 items-center justify-between border-b border-border px-3 py-2">
        <span className="text-xs font-semibold uppercase tracking-wide text-muted-foreground">
          Planned PRs
        </span>
        <Button
          data-testid="pr-stack-planned-pr-panel-close"
          variant="ghost"
          size="sm"
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
      {children}
    </div>
  );
}

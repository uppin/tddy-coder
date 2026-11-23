import type { CSSProperties, ReactNode } from "react";
import { X } from "lucide-react";
import { Button } from "tddy-ui-web";

/** Inline layout, like the Activities sidebar: the dock's width is measured and the list inside only
 *  scrolls if every element above it bounds its height. */
const PANEL_STYLE: CSSProperties = {
  display: "flex",
  flexDirection: "column",
  height: "100%",
  minHeight: 0,
};
const DOCKED_STYLE: CSSProperties = { ...PANEL_STYLE, position: "relative", width: 440, flexShrink: 0 };
const OVERLAY_STYLE: CSSProperties = { ...PANEL_STYLE, position: "absolute", inset: 0, zIndex: 10 };

/**
 * The session's work queue as the session view's right sidebar, beside Activities: a header with a
 * close control over the queue view. Its open state and toggle belong to `SessionWithActivitySidebar`.
 */
export function SessionWorkQueueSidebar({
  mobile,
  onClose,
  children,
}: {
  /** Full-width overlay at mobile width; a docked column otherwise. */
  mobile: boolean;
  onClose: () => void;
  children: ReactNode;
}) {
  return (
    <section
      data-testid="work-queue-sidebar"
      className="border-l border-border bg-background"
      style={mobile ? OVERLAY_STYLE : DOCKED_STYLE}
    >
      <div className="flex flex-shrink-0 items-center justify-between border-b border-border px-3 py-2">
        <span className="text-xs font-semibold uppercase tracking-wide text-muted-foreground">Work queue</span>
        <Button
          data-testid="work-queue-sidebar-close"
          variant="ghost"
          size="icon-sm"
          className="h-6 w-6 p-0"
          onClick={onClose}
          title="Close"
        >
          <X className="h-3.5 w-3.5" />
        </Button>
      </div>
      <div className="flex min-h-0 flex-1 flex-col overflow-y-auto">{children}</div>
    </section>
  );
}

import React, { useState } from "react";
import { ChevronDown, ChevronRight } from "lucide-react";

interface SessionDrawerSeparatorProps {
  /** Partition name shown in the header, e.g. "Active" or "Remaining". */
  label: string;
  /** Number of sessions in the partition, rendered as "{label} ({count})" when given. */
  count?: number;
  /** Whether the partition starts expanded. */
  defaultOpen: boolean;
  /** When true, the body stays visible regardless of the local open state (e.g. bulk selection). */
  forceOpen?: boolean;
  /** `data-testid` for the toggle button. */
  testId: string;
  /** The partition body — rendered (visible) while the header is open or forced open. */
  children: React.ReactNode;
}

/**
 * A collapsible partition header for the open sessions drawer. Mirrors the collapse behaviour of
 * the PR-stack group (a toggle plus a body div hidden via `display: none`), but uses a
 * `<button>` rather than a nested `<details>` so it does not affect drawer `<details>` counts.
 */
export function SessionDrawerSeparator({
  label,
  count,
  defaultOpen,
  forceOpen = false,
  testId,
  children,
}: SessionDrawerSeparatorProps) {
  const [isOpen, setIsOpen] = useState(defaultOpen);
  const visible = isOpen || forceOpen;

  return (
    <div>
      <button
        type="button"
        data-testid={testId}
        aria-expanded={visible}
        onClick={() => setIsOpen((v) => !v)}
        className="flex w-full items-center justify-end gap-1 px-3 py-1 text-xs text-muted-foreground transition-colors hover:text-foreground"
      >
        {/* Right-aligned: the label (with its count), then the arrow at the row's right edge. */}
        <span>{count === undefined ? label : `${label} (${count})`}</span>
        {isOpen ? (
          <ChevronDown className="h-3.5 w-3.5 flex-shrink-0" />
        ) : (
          <ChevronRight className="h-3.5 w-3.5 flex-shrink-0" />
        )}
      </button>
      <div style={visible ? undefined : { display: "none" }}>{children}</div>
    </div>
  );
}

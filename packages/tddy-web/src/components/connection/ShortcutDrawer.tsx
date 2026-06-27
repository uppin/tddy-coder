import React, { useCallback, useRef, useState } from "react";
import { GripVertical } from "lucide-react";
import { keySequenceToBytes, type ToolShortcutDef } from "../../lib/toolShortcuts";

export interface ShortcutDrawerProps {
  shortcuts: ToolShortcutDef[];
  onSend: (bytes: Uint8Array) => void;
  /** @deprecated No longer used — the overlay is now bounded to its terminal container. */
  viewportHeight?: number;
}

/** The overlay snaps to the left or right side of its terminal, keeping its vertical position. */
type SnapEdge = "left" | "right";

interface SnapState {
  edge: SnapEdge;
  /** Vertical position (px from the top of the terminal container). */
  top: number;
}

const MARGIN = 8;
/** Pointer movement (px) beyond which a press is treated as a drag rather than a tap. */
const DRAG_THRESHOLD_PX = 5;
/** Size (px) of the collapsed control. */
const COLLAPSED_PX = 28;

function snapStateToStyle(snap: SnapState): React.CSSProperties {
  const top = Math.max(snap.top, MARGIN);
  // Position by the snapped side (relative to the terminal container) so the panel
  // can never leave the terminal and the exact panel width never matters.
  return snap.edge === "left" ? { left: MARGIN, top } : { right: MARGIN, top };
}

export function ShortcutDrawer({ shortcuts, onSend }: ShortcutDrawerProps) {
  const panelRef = useRef<HTMLDivElement>(null);

  // Collapsed by default — shows only the draggable control; tap it to reveal the shortcuts.
  const [collapsed, setCollapsed] = useState(true);

  // Default position: upper-right of the terminal.
  const [snap, setSnap] = useState<SnapState>({ edge: "right", top: MARGIN });

  const dragRef = useRef<{
    pointerId: number;
    startClientX: number;
    startClientY: number;
    startLeft: number;
    startTop: number;
    moved: boolean;
  } | null>(null);

  /** The positioned container the overlay is bounded to (its offset parent). */
  const boundsOf = (panel: HTMLDivElement): { width: number; height: number } => {
    const parent = panel.offsetParent as HTMLElement | null;
    if (parent) return { width: parent.clientWidth, height: parent.clientHeight };
    return { width: window.innerWidth, height: window.innerHeight };
  };

  const onPointerDown = useCallback((e: React.PointerEvent<HTMLDivElement>) => {
    e.preventDefault();
    const panel = panelRef.current;
    if (!panel) return;
    (e.currentTarget as HTMLDivElement).setPointerCapture(e.pointerId);
    dragRef.current = {
      pointerId: e.pointerId,
      startClientX: e.clientX,
      startClientY: e.clientY,
      startLeft: panel.offsetLeft,
      startTop: panel.offsetTop,
      moved: false,
    };
  }, []);

  const onPointerMove = useCallback((e: React.PointerEvent<HTMLDivElement>) => {
    const drag = dragRef.current;
    if (!drag || drag.pointerId !== e.pointerId) return;
    const dx = e.clientX - drag.startClientX;
    const dy = e.clientY - drag.startClientY;
    // Ignore sub-threshold jitter so a tap isn't mistaken for a drag.
    if (!drag.moved && Math.hypot(dx, dy) < DRAG_THRESHOLD_PX) return;
    drag.moved = true;
    const panel = panelRef.current;
    if (!panel) return;
    const { width: cw, height: ch } = boundsOf(panel);
    const pw = panel.offsetWidth;
    const ph = panel.offsetHeight;
    const newLeft = Math.min(Math.max(drag.startLeft + dx, MARGIN), cw - pw - MARGIN);
    const newTop = Math.min(Math.max(drag.startTop + dy, MARGIN), ch - ph - MARGIN);
    panel.style.left = `${newLeft}px`;
    panel.style.top = `${newTop}px`;
    panel.style.right = "auto";
    panel.style.bottom = "auto";
  }, []);

  const endPointer = useCallback((e: React.PointerEvent<HTMLDivElement>, cancelled: boolean) => {
    const drag = dragRef.current;
    if (!drag || drag.pointerId !== e.pointerId) return;
    dragRef.current = null;
    try {
      (e.currentTarget as HTMLDivElement).releasePointerCapture(e.pointerId);
    } catch {
      /* already released */
    }
    const panel = panelRef.current;
    if (!panel) return;

    // A press without movement is a tap → toggle collapsed (unless the gesture was cancelled).
    if (!drag.moved) {
      if (!cancelled) setCollapsed((c) => !c);
      return;
    }

    // Snap to whichever side the handle is currently in (its center relative to the
    // container midpoint). Vertical position is preserved.
    const { width: cw } = boundsOf(panel);
    const centerX = panel.offsetLeft + panel.offsetWidth / 2;
    const edge: SnapEdge = centerX < cw / 2 ? "left" : "right";
    const top = panel.offsetTop;
    setSnap({ edge, top });
    // Clear inline styles so the snapped position from React takes over.
    panel.style.left = "";
    panel.style.top = "";
    panel.style.right = "";
    panel.style.bottom = "";
  }, []);

  const onPointerUp = useCallback(
    (e: React.PointerEvent<HTMLDivElement>) => endPointer(e, false),
    [endPointer],
  );
  const onPointerCancel = useCallback(
    (e: React.PointerEvent<HTMLDivElement>) => endPointer(e, true),
    [endPointer],
  );

  if (shortcuts.length === 0) return null;

  const posStyle = snapStateToStyle(snap);

  return (
    <div
      ref={panelRef}
      data-testid="shortcut-drawer"
      data-snap-edge={snap.edge}
      data-collapsed={collapsed}
      style={{
        position: "absolute",
        zIndex: 200,
        backgroundColor: "rgba(0,0,0,0.82)",
        border: "1px solid #555",
        borderRadius: 6,
        boxShadow: "0 4px 16px rgba(0,0,0,0.5)",
        display: "flex",
        flexDirection: "column",
        alignItems: "center",
        gap: 4,
        padding: "4px 6px",
        userSelect: "none",
        touchAction: "none",
        maxHeight: `calc(100% - ${2 * MARGIN}px)`,
        overflowY: "auto",
        ...posStyle,
      }}
    >
      <div
        data-testid="shortcut-drag-handle"
        title="Shortcuts — tap to expand, drag to move"
        style={{
          cursor: "grab",
          display: "flex",
          alignItems: "center",
          padding: "0 2px",
          // Without this, a touch starting on the handle is treated as a scroll/pan
          // and the pointer is cancelled before a drag can register (mobile).
          touchAction: "none",
        }}
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
        onPointerCancel={onPointerCancel}
      >
        <GripVertical size={14} style={{ color: "#888" }} aria-hidden />
      </div>
      {!collapsed &&
        shortcuts.map((s) => (
          <button
            key={s.label}
            type="button"
            data-testid={`shortcut-button-${s.label}`}
            onClick={() => {
              const bytes = keySequenceToBytes(s.keys);
              if (bytes.length > 0) onSend(bytes);
            }}
            style={{
              padding: "3px 8px",
              fontSize: 11,
              cursor: "pointer",
              backgroundColor: "rgba(255,255,255,0.1)",
              color: "#ddd",
              border: "1px solid #666",
              borderRadius: 4,
              whiteSpace: "nowrap",
              width: "100%",
            }}
          >
            {s.label}
          </button>
        ))}
    </div>
  );
}

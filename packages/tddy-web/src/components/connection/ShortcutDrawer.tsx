import React, { useCallback, useRef, useState } from "react";
import { GripVertical } from "lucide-react";
import { keySequenceToBytes, type ToolShortcutDef } from "../../lib/toolShortcuts";

export interface ShortcutDrawerProps {
  shortcuts: ToolShortcutDef[];
  onSend: (bytes: Uint8Array) => void;
  /** Visual viewport height in px; 0 = use window.innerHeight. */
  viewportHeight: number;
}

type SnapEdge = "top" | "bottom" | "left" | "right";

interface SnapState {
  edge: SnapEdge;
  /** Position along the edge (left/right for top/bottom snaps; top/bottom for left/right snaps). */
  offset: number;
}

const MARGIN = 8;

function resolveViewportHeight(viewportHeight: number): number {
  if (viewportHeight > 0) return viewportHeight;
  return typeof window !== "undefined" ? window.innerHeight : 600;
}

function snapStateToStyle(
  snap: SnapState,
  panelWidth: number,
  panelHeight: number,
  viewportHeight: number,
): React.CSSProperties {
  const vh = resolveViewportHeight(viewportHeight);
  const vw = typeof window !== "undefined" ? window.innerWidth : 800;

  const clampH = (v: number) => Math.min(Math.max(v, MARGIN), vh - panelHeight - MARGIN);
  const clampW = (v: number) => Math.min(Math.max(v, MARGIN), vw - panelWidth - MARGIN);

  switch (snap.edge) {
    case "top":
      return { top: MARGIN, left: clampW(snap.offset) };
    case "bottom":
      return { top: vh - panelHeight - MARGIN, left: clampW(snap.offset) };
    case "left":
      return { left: MARGIN, top: clampH(snap.offset) };
    case "right":
      return { left: vw - panelWidth - MARGIN, top: clampH(snap.offset) };
  }
}

function nearestEdge(
  centerX: number,
  centerY: number,
  vw: number,
  vh: number,
): SnapEdge {
  const toTop = centerY;
  const toBottom = vh - centerY;
  const toLeft = centerX;
  const toRight = vw - centerX;
  const min = Math.min(toTop, toBottom, toLeft, toRight);
  if (min === toBottom) return "bottom";
  if (min === toTop) return "top";
  if (min === toLeft) return "left";
  return "right";
}

export function ShortcutDrawer({ shortcuts, onSend, viewportHeight }: ShortcutDrawerProps) {
  const panelRef = useRef<HTMLDivElement>(null);

  const [snap, setSnap] = useState<SnapState>(() => {
    const vw = typeof window !== "undefined" ? window.innerWidth : 800;
    return { edge: "bottom", offset: vw / 2 - 60 };
  });

  const dragRef = useRef<{
    pointerId: number;
    startClientX: number;
    startClientY: number;
    startLeft: number;
    startTop: number;
  } | null>(null);

  const onPointerDown = useCallback(
    (e: React.PointerEvent<HTMLDivElement>) => {
      e.preventDefault();
      const panel = panelRef.current;
      if (!panel) return;
      const rect = panel.getBoundingClientRect();
      (e.currentTarget as HTMLDivElement).setPointerCapture(e.pointerId);
      dragRef.current = {
        pointerId: e.pointerId,
        startClientX: e.clientX,
        startClientY: e.clientY,
        startLeft: rect.left,
        startTop: rect.top,
      };
    },
    [],
  );

  const onPointerMove = useCallback(
    (e: React.PointerEvent<HTMLDivElement>) => {
      const drag = dragRef.current;
      if (!drag || drag.pointerId !== e.pointerId) return;
      const dx = e.clientX - drag.startClientX;
      const dy = e.clientY - drag.startClientY;
      const panel = panelRef.current;
      if (!panel) return;
      const vw = window.innerWidth;
      const vh = resolveViewportHeight(viewportHeight);
      const { width: pw, height: ph } = panel.getBoundingClientRect();
      const newLeft = Math.min(Math.max(drag.startLeft + dx, MARGIN), vw - pw - MARGIN);
      const newTop = Math.min(Math.max(drag.startTop + dy, MARGIN), vh - ph - MARGIN);
      panel.style.left = `${newLeft}px`;
      panel.style.top = `${newTop}px`;
      panel.style.bottom = "auto";
      panel.style.right = "auto";
    },
    [viewportHeight],
  );

  const onPointerUp = useCallback(
    (e: React.PointerEvent<HTMLDivElement>) => {
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
      const rect = panel.getBoundingClientRect();
      const vw = window.innerWidth;
      const vh = resolveViewportHeight(viewportHeight);
      const centerX = rect.left + rect.width / 2;
      const centerY = rect.top + rect.height / 2;
      const edge = nearestEdge(centerX, centerY, vw, vh);
      const offset = edge === "top" || edge === "bottom" ? rect.left : rect.top;
      setSnap({ edge, offset });
      // Clear inline styles so CSS takes over from snap state
      panel.style.left = "";
      panel.style.top = "";
      panel.style.bottom = "";
      panel.style.right = "";
    },
    [viewportHeight],
  );

  if (shortcuts.length === 0) return null;

  const isHorizontal = snap.edge === "top" || snap.edge === "bottom";
  const panelWidth = isHorizontal ? shortcuts.length * 80 + 28 : 80;
  const panelHeight = isHorizontal ? 40 : shortcuts.length * 36 + 28;

  const posStyle = snapStateToStyle(snap, panelWidth, panelHeight, viewportHeight);

  return (
    <div
      ref={panelRef}
      data-testid="shortcut-drawer"
      data-snap-edge={snap.edge}
      style={{
        position: "fixed",
        zIndex: 200,
        backgroundColor: "rgba(0,0,0,0.82)",
        border: "1px solid #555",
        borderRadius: 6,
        boxShadow: "0 4px 16px rgba(0,0,0,0.5)",
        display: "flex",
        flexDirection: isHorizontal ? "row" : "column",
        alignItems: "center",
        gap: 4,
        padding: "4px 6px",
        userSelect: "none",
        touchAction: "none",
        ...posStyle,
      }}
    >
      <div
        data-testid="shortcut-drag-handle"
        style={{ cursor: "grab", display: "flex", alignItems: "center", padding: "0 2px" }}
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
        onPointerCancel={onPointerUp}
      >
        <GripVertical
          size={14}
          style={{ color: "#888" }}
          aria-hidden
        />
      </div>
      {shortcuts.map((s) => (
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
          }}
        >
          {s.label}
        </button>
      ))}
    </div>
  );
}

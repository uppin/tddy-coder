import type React from "react";
import { quoted } from "./WorkItemConnectors";
import { useCallback, useEffect, useRef, useState } from "react";

export interface ItemDragState {
  itemId: string;
  /** Index the item would land at among the rendered rows if dropped now. */
  targetIndex: number;
}

/**
 * Pointer-driven drag of one task row by its handle. The drop index is the number of *other* rows
 * whose vertical middle lies above the pointer; releasing calls `onDrop(itemId, delta)` with the
 * signed distance from the row's current index. Rows are located through `rowSelector(id)`.
 */
export function useItemDrag(
  container: HTMLElement | null,
  orderedIds: string[],
  onDrop: (itemId: string, delta: number) => void,
) {
  const [drag, setDrag] = useState<ItemDragState | null>(null);
  const latest = useRef({ orderedIds, onDrop, container });
  latest.current = { orderedIds, onDrop, container };

  const indexAt = useCallback((itemId: string, clientY: number): number => {
    const { orderedIds: ids, container: root } = latest.current;
    let above = 0;
    for (const id of ids) {
      if (id === itemId) continue;
      const el = root?.querySelector<HTMLElement>(`[data-testid="work-item-row-${quoted(id)}"]`);
      if (!el) continue;
      const rect = el.getBoundingClientRect();
      if (rect.top + rect.height / 2 < clientY) above += 1;
    }
    return above;
  }, []);

  const dragId = drag?.itemId ?? null;
  useEffect(() => {
    if (dragId === null) return;
    const move = (e: PointerEvent) => setDrag({ itemId: dragId, targetIndex: indexAt(dragId, e.clientY) });
    const up = (e: PointerEvent) => {
      const { orderedIds: ids, onDrop: drop } = latest.current;
      const delta = indexAt(dragId, e.clientY) - ids.indexOf(dragId);
      setDrag(null);
      if (delta !== 0) drop(dragId, delta);
    };
    const cancel = () => setDrag(null);
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
    window.addEventListener("pointercancel", cancel);
    return () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      window.removeEventListener("pointercancel", cancel);
    };
  }, [dragId, indexAt]);

  const start = (itemId: string) => (e: React.PointerEvent) => {
    if (e.button !== 0) return;
    e.preventDefault();
    setDrag({ itemId, targetIndex: orderedIds.indexOf(itemId) });
  };
  return { drag, start };
}

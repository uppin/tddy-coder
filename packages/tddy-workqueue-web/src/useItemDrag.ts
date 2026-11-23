import type React from "react";
import { quoted } from "./WorkItemConnectors";
import { useCallback, useEffect, useRef, useState } from "react";

export interface ItemDragState {
  itemId: string;
  /** Index the item would land at among the rendered rows if dropped now. */
  targetIndex: number;
  /** Where the drop line sits, in the container's coordinates. */
  indicatorY: number;
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
  const startYRef = useRef(0);
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

  const rowOf = useCallback(
    (id: string) =>
      latest.current.container?.querySelector<HTMLElement>(`[data-testid="work-item-row-${quoted(id)}"]`) ?? null,
    [],
  );
  /** The line between the other rows where a drop at `index` would land. */
  const indicatorAt = useCallback(
    (itemId: string, index: number): number => {
      const others = latest.current.orderedIds.filter((id) => id !== itemId).map(rowOf).filter(Boolean) as HTMLElement[];
      if (others.length === 0) return 0;
      if (index <= 0) return others[0].offsetTop - 6;
      const prev = others[Math.min(index, others.length) - 1];
      return prev.offsetTop + prev.offsetHeight + 6;
    },
    [rowOf],
  );

  const dragId = drag?.itemId ?? null;
  useEffect(() => {
    if (dragId === null) return;
    const el = rowOf(dragId);
    const startY = startYRef.current;
    if (el) Object.assign(el.style, { position: "relative", zIndex: "20", opacity: "0.85", boxShadow: "0 6px 16px rgba(0,0,0,0.25)", cursor: "grabbing" });
    const reset = () => {
      if (el) Object.assign(el.style, { transform: "", position: "", zIndex: "", opacity: "", boxShadow: "", cursor: "" });
    };
    const move = (e: PointerEvent) => {
      if (el) el.style.transform = `translateY(${e.clientY - startY}px)`;
      const targetIndex = indexAt(dragId, e.clientY);
      setDrag({ itemId: dragId, targetIndex, indicatorY: indicatorAt(dragId, targetIndex) });
    };
    const up = (e: PointerEvent) => {
      const { orderedIds: ids, onDrop: drop } = latest.current;
      const delta = indexAt(dragId, e.clientY) - ids.indexOf(dragId);
      reset();
      setDrag(null);
      if (delta !== 0) drop(dragId, delta);
    };
    const cancel = () => {
      reset();
      setDrag(null);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
    window.addEventListener("pointercancel", cancel);
    return () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      window.removeEventListener("pointercancel", cancel);
      reset();
    };
  }, [dragId, indexAt, indicatorAt, rowOf]);

  const start = (itemId: string) => (e: React.PointerEvent) => {
    if (e.button !== 0) return;
    e.preventDefault();
    startYRef.current = e.clientY;
    setDrag({ itemId, targetIndex: orderedIds.indexOf(itemId), indicatorY: -1000 });
  };
  return { drag, start };
}

/**
 * Map a viewport point to 1-based terminal cell coordinates for SGR mouse reporting.
 * Uses the **screen** grid (`.xterm-screen`, not the outer container): the cell grid is exactly
 * that element while the wrapper may be wider/taller.
 */

export type TerminalGridRect = {
  left: number;
  top: number;
  width: number;
  height: number;
};

export function clientPointToTerminalCell(
  clientX: number,
  clientY: number,
  gridRect: TerminalGridRect,
  cols: number,
  rows: number
): { col: number; row: number } | null {
  if (cols <= 0 || rows <= 0) return null;
  const { left, top, width, height } = gridRect;
  if (!(width > 0) || !(height > 0)) return null;
  const offsetX = clientX - left;
  const offsetY = clientY - top;
  const cellW = width / cols;
  const cellH = height / rows;
  const col = Math.floor(offsetX / cellW) + 1;
  const row = Math.floor(offsetY / cellH) + 1;
  return {
    col: Math.max(1, Math.min(col, cols)),
    row: Math.max(1, Math.min(row, rows)),
  };
}

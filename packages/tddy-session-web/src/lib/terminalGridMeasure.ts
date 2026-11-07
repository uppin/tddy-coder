/**
 * Estimate terminal cols×rows from a container's pixel size.
 * Matches GrpcSessionTerminal / StreamTerminalOutput initial dimension math.
 */

export const TERMINAL_CHAR_WIDTH_PX = 8;
export const TERMINAL_CHAR_HEIGHT_PX = 17;

export function measureTerminalGridFromRect(
  rect: DOMRectReadOnly | undefined | null,
): { widthPx: number; heightPx: number; cols: number; rows: number } {
  const widthPx = rect && rect.width > 0 ? rect.width : 0;
  const heightPx = rect && rect.height > 0 ? rect.height : 0;
  const cols = widthPx > 0 ? Math.max(1, Math.floor(widthPx / TERMINAL_CHAR_WIDTH_PX)) : 0;
  const rows = heightPx > 0 ? Math.max(1, Math.floor(heightPx / TERMINAL_CHAR_HEIGHT_PX)) : 0;
  return { widthPx, heightPx, cols, rows };
}

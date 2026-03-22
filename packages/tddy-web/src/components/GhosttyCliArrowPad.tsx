import React from "react";
import { bytesForArrowDirection, type CliArrowDirection } from "./ghosttyCliArrowCsi";

export interface GhosttyCliArrowPadProps {
  /** When true, render the arrow pad (bottom-right of the terminal region). */
  visible: boolean;
  /** Same enqueue path as Ghostty `onData` / mobile keyboard. */
  onEnqueue: (encoded: Uint8Array) => void;
  /** When true, log pad interactions (same pattern as GhosttyTerminalLiveKit `debugLogging`). */
  debugLogging?: boolean;
}

const PAD_PANEL: React.CSSProperties = {
  position: "absolute",
  bottom: 16,
  right: 16,
  zIndex: 101,
  display: "grid",
  gridTemplateColumns: "44px 44px 44px",
  gridTemplateRows: "44px 44px 44px",
  gap: 4,
  padding: 8,
  backgroundColor: "rgba(0,0,0,0.7)",
  border: "1px solid #555",
  borderRadius: 8,
  pointerEvents: "auto",
};

const ARROW_BTN: React.CSSProperties = {
  minWidth: 44,
  minHeight: 44,
  padding: 0,
  fontSize: 18,
  lineHeight: 1,
  cursor: "pointer",
  backgroundColor: "rgba(0,0,0,0.6)",
  color: "#eee",
  border: "1px solid #666",
  borderRadius: 4,
};

const ARROWS: {
  direction: CliArrowDirection;
  testId: string;
  label: string;
  gridColumn: number;
  gridRow: number;
  symbol: string;
}[] = [
  { direction: "up", testId: "ghostty-cli-arrow-up", label: "Send terminal arrow up (CSI Up)", gridColumn: 2, gridRow: 1, symbol: "↑" },
  { direction: "left", testId: "ghostty-cli-arrow-left", label: "Send terminal arrow left (CSI Left)", gridColumn: 1, gridRow: 2, symbol: "←" },
  { direction: "right", testId: "ghostty-cli-arrow-right", label: "Send terminal arrow right (CSI Right)", gridColumn: 3, gridRow: 2, symbol: "→" },
  { direction: "down", testId: "ghostty-cli-arrow-down", label: "Send terminal arrow down (CSI Down)", gridColumn: 2, gridRow: 3, symbol: "↓" },
];

/**
 * Bottom-right arrow pad for CLI mode (touch targets, CSI via shared enqueue).
 */
export function GhosttyCliArrowPad({
  visible,
  onEnqueue,
  debugLogging = false,
}: GhosttyCliArrowPadProps) {
  if (!visible) return null;

  return (
    <div
      data-testid="ghostty-cli-arrow-pad"
      style={PAD_PANEL}
      aria-label="On-screen terminal arrow keys"
    >
      {ARROWS.map(({ direction, testId, label, gridColumn, gridRow, symbol }) => (
        <button
          key={direction}
          type="button"
          data-testid={testId}
          aria-label={label}
          style={{
            ...ARROW_BTN,
            gridColumn,
            gridRow,
          }}
          onClick={(ev) => {
            ev.preventDefault();
            ev.stopPropagation();
            const encoded = bytesForArrowDirection(direction);
            if (debugLogging) {
              console.debug("[GhosttyCliArrowPad] arrow tap", {
                direction,
                bytes: Array.from(encoded),
              });
              console.info("[GhosttyCliArrowPad] enqueue CSI arrow", {
                direction,
                byteLength: encoded.length,
              });
            }
            onEnqueue(encoded);
          }}
        >
          {symbol}
        </button>
      ))}
    </div>
  );
}

/**
 * Shared VT CSI arrow byte sequences for Ghostty CLI mode and keyboard paths.
 * Must stay aligned with `handleMobileKeyDown` (uses `bytesForArrowDirection` for Arrow keys).
 */

export type CliArrowDirection = "up" | "down" | "left" | "right";

const CSI_ARROW_BYTES: Record<CliArrowDirection, readonly number[]> = {
  up: [0x1b, 0x5b, 0x41],
  down: [0x1b, 0x5b, 0x42],
  right: [0x1b, 0x5b, 0x43],
  left: [0x1b, 0x5b, 0x44],
};

/**
 * Returns CSI bytes for an arrow direction (ESC [ A/B/C/D).
 * Returns a fresh buffer so callers cannot mutate shared state.
 */
export function bytesForArrowDirection(
  direction: CliArrowDirection
): Uint8Array {
  const raw = CSI_ARROW_BYTES[direction];
  return new Uint8Array(raw);
}

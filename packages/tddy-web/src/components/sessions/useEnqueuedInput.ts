import { useCallback, useEffect, useRef, useState } from "react";
import {
  TerminalInputQueue,
  type OverlayModel,
} from "../../lib/terminalInputQueue";

/**
 * Owns a {@link TerminalInputQueue} and the "show after N ms without an ACK" visibility rule for
 * the enqueued-input overlay (docs/ft/web/enqueued-input-overlay.md).
 *
 * `enqueue` records a sent chunk and returns its cumulative byte offset (the caller sends that on
 * `SendTerminalInput`). `ack` feeds back the daemon's `acked_input_offset`. The overlay becomes
 * `visible` only once input has been outstanding longer than `delayMs`, stays visible while any
 * byte is un-acked (re-flowing as ACKs collapse it), and hides the moment the queue drains — so on
 * a fast link, where ACKs beat the threshold, it never appears.
 */

const DEFAULT_OVERLAY_DELAY_MS = 500;
const DEFAULT_MAX_ITEMS = 40;

export interface UseEnqueuedInputOptions {
  /** Milliseconds an un-acked byte may be outstanding before the overlay appears. */
  delayMs?: number;
  /** Maximum display items on the single line before the rest collapse into an overflow count. */
  maxItems?: number;
}

export interface EnqueuedInput {
  /** Record a sent chunk; returns its cumulative end offset (to send as `input_offset`). */
  enqueue: (bytes: Uint8Array) => number;
  /** Apply the daemon's acknowledged input offset. */
  ack: (offset: number) => void;
  /** Current single-line overlay model for the un-acked remainder. */
  model: OverlayModel;
  /** Whether the overlay should be shown. */
  visible: boolean;
}

export function useEnqueuedInput({
  delayMs = DEFAULT_OVERLAY_DELAY_MS,
  maxItems = DEFAULT_MAX_ITEMS,
}: UseEnqueuedInputOptions = {}): EnqueuedInput {
  const queueRef = useRef<TerminalInputQueue>(new TerminalInputQueue());
  const [visible, setVisible] = useState(false);
  const [, setTick] = useState(0);
  const bump = useCallback(() => setTick((t) => t + 1), []);

  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const visibleRef = useRef(false);
  visibleRef.current = visible;

  const clearTimer = useCallback(() => {
    if (timerRef.current !== null) {
      clearTimeout(timerRef.current);
      timerRef.current = null;
    }
  }, []);

  const enqueue = useCallback(
    (bytes: Uint8Array): number => {
      const endOffset = queueRef.current.enqueue(bytes);
      // Start the reveal timer when input first becomes outstanding; a single timer covers the run.
      if (!visibleRef.current && timerRef.current === null && queueRef.current.hasUnacked()) {
        timerRef.current = setTimeout(() => {
          timerRef.current = null;
          if (queueRef.current.hasUnacked()) setVisible(true);
        }, delayMs);
      }
      bump();
      return endOffset;
    },
    [delayMs, bump],
  );

  const ack = useCallback(
    (offset: number): void => {
      queueRef.current.ack(offset);
      if (!queueRef.current.hasUnacked()) {
        clearTimer();
        setVisible(false);
      }
      bump();
    },
    [clearTimer, bump],
  );

  useEffect(() => clearTimer, [clearTimer]);

  const model = queueRef.current.overlayModel(maxItems);
  return { enqueue, ack, model, visible };
}

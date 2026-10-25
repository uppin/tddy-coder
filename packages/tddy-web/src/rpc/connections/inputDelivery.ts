/**
 * Whether a terminal's input reaches its PTY, reported in send order.
 *
 * Each keystroke, paste or resize is its own `SendTerminalInput`, and their answers can arrive in
 * any order. Reporting each answer as it lands would let a refusal of an *earlier* send, answered
 * late, overwrite the success of a later one — an indicator saying input is not delivered while it
 * is. A sequence numbers each send and drops an answer older than the newest one already reported.
 */

/** Report one send's outcome: `null` when it was delivered, else why not. */
export type InputDeliverySettle = (failure: string | null) => void;

/**
 * Start a delivery sequence over `report`. Call the returned function once per send, *as it is
 * sent*, and settle what it returns once that send is answered.
 */
export function inputDeliverySequence(
  report: (failure: string | null) => void,
): () => InputDeliverySettle {
  let issued = 0;
  let latestSettled = 0;
  return () => {
    const seq = ++issued;
    return (failure) => {
      if (seq < latestSettled) return;
      latestSettled = seq;
      report(failure);
    };
  };
}

/** The part of an output frame that reports on input delivery. */
export interface InputDeliveryFrame {
  readonly refusedInputOffset: bigint;
  readonly inputRefusal: string;
  readonly ackedInputOffset: bigint;
}

/**
 * Turns the daemon's refusal and ACK frames into delivery reports.
 *
 * A refusal frame names the chunk (by its cumulative input offset) that was dropped and why; the
 * indicator stays until an ACK reaches past that offset, which means a later chunk was applied. An
 * ACK at or below it says nothing about the refused bytes and clears nothing.
 */
export class InputRefusalTracker {
  private refusedAt: bigint | null = null;
  private readonly nextDelivery: () => InputDeliverySettle;

  constructor(report: (failure: string | null) => void) {
    this.nextDelivery = inputDeliverySequence(report);
  }

  /** Account for one frame; true when it was a refusal (it carries no output). */
  observe(frame: InputDeliveryFrame): boolean {
    if (frame.inputRefusal !== "") {
      this.refusedAt = frame.refusedInputOffset;
      this.nextDelivery()(frame.inputRefusal);
      return true;
    }
    if (this.refusedAt !== null && frame.ackedInputOffset > this.refusedAt) {
      this.refusedAt = null;
      this.nextDelivery()(null);
    }
    return false;
  }
}

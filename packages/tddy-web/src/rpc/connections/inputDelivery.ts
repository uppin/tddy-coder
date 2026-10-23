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

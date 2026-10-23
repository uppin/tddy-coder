import { useCallback, useState } from "react";
import { rpcFailureText } from "../rpc/rpcFailure";

export interface ActionFailure {
  /** Why the last action failed, ready to render; `null` while nothing has failed. */
  readonly error: string | null;
  /** True while an action started by {@link ActionFailure.run} has not settled. */
  readonly pending: boolean;
  /**
   * Run `action`, clearing the previous failure first and recording this one if it rejects.
   * Resolves `true` on success and `false` on failure — it never rejects, so a caller has no
   * failure left to swallow.
   */
  run(action: () => Promise<unknown>): Promise<boolean>;
  /** Record a failure that did not come through {@link ActionFailure.run} (a load, a stream). */
  report(error: unknown): void;
  clear(): void;
}

/** One surface's failure slot: what the operator last tried, and why it did not work. */
export function useActionFailure(): ActionFailure {
  const [error, setError] = useState<string | null>(null);
  const [pending, setPending] = useState(false);

  const run = useCallback(async (action: () => Promise<unknown>) => {
    setError(null);
    setPending(true);
    try {
      await action();
      return true;
    } catch (e) {
      setError(rpcFailureText(e));
      return false;
    } finally {
      setPending(false);
    }
  }, []);
  const report = useCallback((e: unknown) => setError(rpcFailureText(e)), []);
  const clear = useCallback(() => setError(null), []);

  return { error, pending, run, report, clear };
}

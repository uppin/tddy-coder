import { useCallback, useState } from "react";
import { rpcFailureText } from "../rpc/rpcFailure";

export interface KeyedActionFailure {
  /** Why each row's last action failed, keyed by row; a row with no entry has not failed. */
  readonly errors: Readonly<Record<string, string>>;
  /**
   * Record why `key`'s action failed. `describe` turns the error into the text shown — the shared
   * {@link rpcFailureText} unless a surface has its own vocabulary for it.
   */
  report(key: string, error: unknown, describe?: (error: unknown) => string): void;
  /** Forget `key`'s failure — the row is being retried, or is gone. */
  clear(key: string): void;
}

/**
 * A failure slot per row: the per-row counterpart of `useActionFailure`, for a list whose rows each
 * carry their own action (start, remove, delete) and say beside themselves why it did not work.
 */
export function useKeyedActionFailure(): KeyedActionFailure {
  const [errors, setErrors] = useState<Record<string, string>>({});

  const report = useCallback(
    (key: string, error: unknown, describe: (error: unknown) => string = rpcFailureText) =>
      setErrors((held) => ({ ...held, [key]: describe(error) })),
    [],
  );
  const clear = useCallback(
    (key: string) =>
      setErrors((held) => {
        if (!(key in held)) return held;
        const next = { ...held };
        delete next[key];
        return next;
      }),
    [],
  );

  return { errors, report, clear };
}

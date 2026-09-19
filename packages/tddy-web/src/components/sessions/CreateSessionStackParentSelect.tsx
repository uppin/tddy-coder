import type { SessionEntry } from "../../gen/session_pb";
import { inputClass, labelClass } from "./createSessionFormStyles";

export interface CreateSessionStackParentSelectProps {
  /** The orchestrators this session can be stack-parented to. Empty renders nothing. */
  stackParentOptions: SessionEntry[];
  stackParent: string;
  setStackParent: (stackParent: string) => void;
}

/** PR stack parent picker — shown for both session types when orchestrators are available. */
export function CreateSessionStackParentSelect({
  stackParentOptions,
  stackParent,
  setStackParent,
}: CreateSessionStackParentSelectProps) {
  return (
    <>
      {stackParentOptions.length > 0 && (
        <div>
          <label className={labelClass} htmlFor="create-session-stack-parent">
            PR stack parent
          </label>
          <select
            id="create-session-stack-parent"
            data-testid="create-session-stack-parent-select"
            className={inputClass}
            value={stackParent}
            onChange={(e) => setStackParent(e.target.value)}
          >
            <option value="">None (standalone session)</option>
            {stackParentOptions.map((s) => (
              <option key={s.sessionId} value={s.sessionId}>
                {s.sessionId}
              </option>
            ))}
          </select>
        </div>
      )}
    </>
  );
}

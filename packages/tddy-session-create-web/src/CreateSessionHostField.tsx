import type { DaemonHost } from "tddy-client-web";
import { inputClass, labelClass } from "./createSessionFormStyles";

export interface CreateSessionHostFieldProps {
  daemons: DaemonHost[];
  daemonInstanceId: string;
  setDaemonInstanceId: (daemonInstanceId: string) => void;
}

/**
 * **Host** — which daemon runs the session, test id `create-session-host-select`; shown only when
 * the common room advertises daemons. Split out of `CreateSessionHostAndProjectFields` so the
 * orchestrator composer (`#orchestrators` 8/11) offers the same picker without the project select.
 * Presentational: the value stays with the caller.
 */
export function CreateSessionHostField({ daemons, daemonInstanceId, setDaemonInstanceId }: CreateSessionHostFieldProps) {
  if (daemons.length === 0) return null;
  return (
    <div>
      <label className={labelClass} htmlFor="create-session-host">
        Host
      </label>
      <select
        id="create-session-host"
        data-testid="create-session-host-select"
        className={inputClass}
        value={daemonInstanceId}
        onChange={(e) => setDaemonInstanceId(e.target.value)}
      >
        {daemons.map((d) => (
          <option key={d.instanceId} value={d.instanceId}>
            {d.label}
          </option>
        ))}
      </select>
    </div>
  );
}

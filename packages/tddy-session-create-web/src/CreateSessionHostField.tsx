import type { DaemonHost } from "tddy-client-web";
import { safeTestIdPart, useDerivedHostListingFailures } from "tddy-client-web";
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
 *
 * Hosts derived from a daemon — machines it reaches through a remote shell — are offered in a
 * group under that daemon. Choosing one still holds its own id as the value; the caller turns it
 * into the parent as `daemonInstanceId` plus the host's `target` on the request.
 */
export function CreateSessionHostField({ daemons, daemonInstanceId, setDaemonInstanceId }: CreateSessionHostFieldProps) {
  const failures = useDerivedHostListingFailures();
  if (daemons.length === 0) return null;
  const parents = daemons.filter((d) => !d.parent);
  const selected = daemons.find((d) => d.instanceId === daemonInstanceId);
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
        {parents.map((d) => (
          <option key={d.instanceId} value={d.instanceId}>
            {d.label}
          </option>
        ))}
        {parents.map((parent) => {
          const derived = daemons.filter((d) => d.parent === parent.instanceId);
          if (derived.length === 0) return null;
          return (
            <optgroup
              key={parent.instanceId}
              data-testid={`create-session-host-derived-group-${safeTestIdPart(parent.instanceId)}`}
              label={parent.label}
            >
              {derived.map((d) => {
                const unusable = d.usable === false;
                return (
                  <option key={d.instanceId} value={d.instanceId} disabled={unusable}>
                    {unusable && d.unusableReason
                      ? `${d.label} — ${d.unusableReason}`
                      : d.label}
                  </option>
                );
              })}
            </optgroup>
          );
        })}
      </select>
      {failures.map((failure) => (
        <p
          key={failure.parentHostId}
          data-testid={`create-session-derived-hosts-failure-${safeTestIdPart(failure.parentHostId)}`}
          className="mt-1 text-xs text-destructive"
        >
          Could not list the hosts {failure.parentHostId} reaches: {failure.reason}
        </p>
      ))}
      {selected?.parent ? (
        <p data-testid="create-session-derived-host-note" className="mt-1 text-xs text-muted-foreground">
          This host is reached through {selected.parent}. Its agent runs there with no tddy
          software installed, so permission prompts are answered in the terminal.
        </p>
      ) : null}
    </div>
  );
}

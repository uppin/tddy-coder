import type { DaemonHost } from "../../lib/participantRole";
import type { ProjectEntry } from "../../gen/project_pb";
import type { projectSelectOptions } from "../../lib/projectSelectOptions";
import { inputClass, labelClass } from "./createSessionFormStyles";

export interface CreateSessionHostAndProjectFieldsProps {
  daemons: DaemonHost[];
  daemonInstanceId: string;
  setDaemonInstanceId: (daemonInstanceId: string) => void;
  projects: ProjectEntry[];
  projectOptions: ReturnType<typeof projectSelectOptions>;
  projectId: string;
  setProjectId: (projectId: string) => void;
}

/**
 * **Host** — which daemon runs the session, shown only when the common room advertises daemons —
 * and **Project**, the repository it is started in. The two selects every session type shows,
 * above the per-type fields. Presentational: both values stay in `CreateSessionPane`.
 */
export function CreateSessionHostAndProjectFields({
  daemons,
  daemonInstanceId,
  setDaemonInstanceId,
  projects,
  projectOptions,
  projectId,
  setProjectId,
}: CreateSessionHostAndProjectFieldsProps) {
  return (
    <>
      {/* Host — which daemon runs the session. Only shown when the common room advertises daemons. */}
      {daemons.length > 0 && (
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
      )}

      <div>
        <label className={labelClass} htmlFor="create-session-project">
          Project
        </label>
        <select
          id="create-session-project"
          data-testid="create-session-project-select"
          className={inputClass}
          value={projectId}
          onChange={(e) => setProjectId(e.target.value)}
        >
          <option value="" disabled>
            {projects.length === 0 ? "No projects available" : "Select a project…"}
          </option>
          {projectOptions.map((option) => (
            <option key={option.projectId} value={option.projectId}>
              {option.label}
            </option>
          ))}
        </select>
      </div>
    </>
  );
}

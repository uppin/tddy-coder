/**
 * Create-session SSH Host alias picker — co-located sessions, Host A.
 *
 * Empty value is LocalShell (this host). A listed alias is RemoteShell on that OpenSSH Host.
 * The list comes from `ListSshConfigHosts` on the session host (n4 retargets this to the
 * codebase host). Wave 2 publishes the control; fetching aliases is still TODO(exec).
 */

import { inputClass, labelClass } from "./createSessionFormStyles";

export function CreateSessionSshConfigSelect() {
  // TODO(exec): implement — ListSshConfigHosts for daemonInstanceId (Host A), distinguish
  // empty vs failed, put the chosen alias on StartSession.ssh_config_host.
  return (
    <div>
      <label className={labelClass} htmlFor="create-session-ssh-config">
        SSH host
      </label>
      <select
        id="create-session-ssh-config"
        data-testid="create-session-ssh-config-select"
        className={inputClass}
        value=""
        onChange={() => {
          /* TODO(exec): implement */
        }}
      >
        <option value="">This host</option>
      </select>
    </div>
  );
}

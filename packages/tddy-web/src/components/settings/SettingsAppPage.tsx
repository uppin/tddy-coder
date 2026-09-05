/**
 * Route container for the daemon settings screen (`#/settings`).
 *
 * The daemon whose settings these are is the one serving this page, so the screen is given the
 * page's own daemon client (`useHttpClient` — same-origin `/rpc` in a browser, the host
 * application's IPC bridge in the desktop app) rather than a common-room client addressing a
 * remote daemon. A daemon reached over LiveKit is somebody else's process; its YAML is edited from
 * its own UI.
 */

import { AppShell } from "../shell/AppShell";
import { useAuthContext } from "../../hooks/authProvider";
import { useHttpClient } from "../../rpc/transportProvider";
import { DaemonConfigService } from "../../gen/daemon_config_pb";
import { DaemonSettingsScreen } from "./DaemonSettingsScreen";

export function SettingsAppPage({ onNavigate }: { onNavigate: (path: string) => void }) {
  const { sessionToken } = useAuthContext();
  const client = useHttpClient(DaemonConfigService);

  return (
    <AppShell title="Settings" onNavigate={onNavigate} variant="scroll">
      <DaemonSettingsScreen client={client} sessionToken={sessionToken ?? ""} />
    </AppShell>
  );
}

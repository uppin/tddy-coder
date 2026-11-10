/**
 * The Hosts page: the application shell around `tddy-hosts-web`'s data container.
 */

import { HostsContent } from "tddy-hosts-web";
import { AppShell } from "../shell/AppShell";

export function HostsAppPage({ onNavigate }: { onNavigate: (path: string) => void }) {
  return (
    <AppShell title="Hosts" onNavigate={onNavigate} dataTestId="hosts-app-page">
      <HostsContent />
    </AppShell>
  );
}

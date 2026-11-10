import { AccountsContent } from "tddy-workspace-web";
import { AppShell } from "../shell/AppShell";

/** The Accounts route: the application shell around the workspace package's accounts content. */
export function AccountsAppPage({ onNavigate }: { onNavigate: (path: string) => void }) {
  return (
    <AppShell title="Accounts" onNavigate={onNavigate} dataTestId="accounts-app-page">
      <AccountsContent />
    </AppShell>
  );
}

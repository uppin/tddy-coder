/**
 * Data container for the Accounts screen: one `ListAccounts` call against the selected daemon.
 *
 * One RPC, deliberately — a vault's contents change only when somebody links, renames or removes an
 * account, and each of those is an action this screen already knows it took.
 */

import { AppShell } from "../shell/AppShell";
import { AccountsScreen, type AccountsOutcome } from "./AccountsScreen";

// TODO(#keyring 4/9): call `AccountsService.listAccounts` through `useDaemonClient`, map
// `vault_locked` and a failed call onto the two non-`listed` outcomes, and wire rename/remove to
// `SetAccountLabel` / `RemoveAccount`. Published as surface; the body lands in this same PR.
const NOT_YET_FETCHED: AccountsOutcome = { kind: "listed", providers: [] };

export function AccountsAppPage({ onNavigate }: { onNavigate: (path: string) => void }) {
  return (
    <AppShell title="Accounts" onNavigate={onNavigate} dataTestId="accounts-app-page">
      <AccountsScreen outcome={NOT_YET_FETCHED} onRename={() => {}} onRemove={() => {}} />
    </AppShell>
  );
}

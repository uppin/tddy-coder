/**
 * Data container for the Accounts screen: one `ListAccounts` call against the selected daemon, plus
 * the `BeginLinkAccount` / `PollLinkAccount` pair behind the add-account control.
 *
 * One read RPC, deliberately — a vault's contents change only when somebody links, renames or
 * removes an account, and each of those is an action this screen already knows it took.
 *
 * **Nothing here signs anybody in.** The link pair is on `AccountsService` rather than
 * `AuthService`, and neither response has a token field, so completing a link cannot replace the
 * session this page is already reading the vault with.
 */

import { AppShell } from "../shell/AppShell";
import { AccountsScreen, type AccountsOutcome } from "./AccountsScreen";

// TODO(#keyring 4/9): call `AccountsService.listAccounts` through `useDaemonClient`, map
// `vault_locked` and a failed call onto the two non-`listed` outcomes, and wire rename/remove to
// `SetAccountLabel` / `RemoveAccount`. Published as surface; the body lands in this same PR.
const NOT_YET_FETCHED: AccountsOutcome = { kind: "listed", providers: [] };

// TODO(#keyring 8/9): carry `session_account` through onto the outcome, call `BeginLinkAccount`
// from `onAddAccount`, poll `PollLinkAccount` at the interval the daemon named, and re-read the
// listing once a link reports `LINK_LINKED`.
export function AccountsAppPage({ onNavigate }: { onNavigate: (path: string) => void }) {
  return (
    <AppShell title="Accounts" onNavigate={onNavigate} dataTestId="accounts-app-page">
      <AccountsScreen
        outcome={NOT_YET_FETCHED}
        onRename={() => {}}
        onRemove={() => {}}
        onAddAccount={() => {}}
      />
    </AppShell>
  );
}

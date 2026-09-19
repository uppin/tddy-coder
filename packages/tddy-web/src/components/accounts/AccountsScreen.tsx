/**
 * Accounts screen — every credential a daemon holds, grouped by provider.
 *
 * Presentational: it renders what it is given, matching the `HostsScreen` / `HostsAppPage` split.
 * `AccountsAppPage` owns the `ListAccounts` call and the three outcomes it can come back with.
 *
 * **No row ever carries a secret.** `accounts.proto` has no field to put one in; `hasSecret` is the
 * single bit that tells a linked account from a stale row.
 */

export interface AccountRow {
  /** Stable within its provider, and what `#keyring` 5/9 assigns to a project. Never renamed. */
  accountId: string;
  /** What a human calls this account — the only mutable field. */
  label: string;
  /** The provider's own identifier, shown beside the label. Never a credential. */
  subject: string;
  updatedAtUnixSeconds: bigint;
  /** Whether a usable credential is present. One bit, not the secret. */
  hasSecret: boolean;
}

export interface ProviderGroup {
  provider: string;
  accounts: AccountRow[];
}

/**
 * What the daemon came back with. The three are held apart deliberately: an opened-and-empty vault,
 * a vault whose key no longer unwraps, and a read that failed are different facts, and rendering
 * any two of them the same way would tell a person to re-link accounts they already have.
 */
export type AccountsOutcome =
  | { kind: "listed"; providers: ProviderGroup[] }
  | { kind: "locked" }
  | { kind: "error"; reason: string };

export interface AccountsScreenProps {
  outcome: AccountsOutcome;
  onRename: (provider: string, accountId: string, label: string) => void;
  onRemove: (provider: string, accountId: string) => void;
}

// TODO(#keyring 4/9): render the grouped rows, the locked notice, the error and the two actions.
// Published as surface so the acceptance spec compiles against the real props; the body lands in
// this same PR's green phase.
export function AccountsScreen(_props: AccountsScreenProps) {
  return <div data-testid="accounts-screen" />;
}

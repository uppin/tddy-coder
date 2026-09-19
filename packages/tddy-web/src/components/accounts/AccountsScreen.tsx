/**
 * Accounts screen — every credential a daemon holds, grouped by provider.
 *
 * Presentational: it renders what it is given, matching the `HostsScreen` / `HostsAppPage` split.
 * `AccountsAppPage` owns the `ListAccounts` call and the three outcomes it can come back with, and
 * the `BeginLinkAccount` / `PollLinkAccount` pair behind the add-account control.
 *
 * **No row ever carries a secret.** `accounts.proto` has no field to put one in; `hasSecret` is the
 * single bit that tells a linked account from a stale row. The same holds through the link flow:
 * what a person sees of it is a short code and where to type it, and the credential it produces
 * never comes back up the wire.
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
 * The account this session was established with.
 *
 * Carried beside the groups rather than as a flag on a row, mirroring `ListAccountsResponse`: it is
 * a fact about *the caller*, and the same record is an ordinary linked account to a daemon that
 * received it through `#keyring` 6/9's propagation.
 */
export interface SessionAccountRef {
  provider: string;
  accountId: string;
}

/**
 * What the daemon came back with. The three are held apart deliberately: an opened-and-empty vault,
 * a vault whose key no longer unwraps, and a read that failed are different facts, and rendering
 * any two of them the same way would tell a person to re-link accounts they already have.
 */
export type AccountsOutcome =
  | { kind: "listed"; providers: ProviderGroup[]; sessionAccount?: SessionAccountRef }
  | { kind: "locked" }
  | { kind: "error"; reason: string };

/**
 * Where an attempt to add another account stands.
 *
 * `denied` and `locked` are separate states for the same reason `LinkState` keeps them apart: the
 * operator refusing at the provider and this daemon having nowhere to put the result are unrelated
 * failures, and only one of them is about a decision somebody made.
 */
export type LinkAttempt =
  | { kind: "awaiting"; provider: string; userCode: string; verificationUri: string }
  | { kind: "denied" }
  | { kind: "expired" }
  | { kind: "locked" };

export interface AccountsScreenProps {
  outcome: AccountsOutcome;
  /** Set while an add-account attempt is in flight or has just ended. */
  linkAttempt?: LinkAttempt;
  onRename: (provider: string, accountId: string, label: string) => void;
  onRemove: (provider: string, accountId: string) => void;
  /** Begin adding another account at this provider. Never signs anybody in. */
  onAddAccount: (provider: string) => void;
}

// TODO(#keyring 4/9): render the grouped rows, the locked notice, the error and the two actions.
// TODO(#keyring 8/9): render the add-account control per provider, the attempt's code and
// verification link, its four end states, and the marker on the account this session was
// established with — whose remove control is refused rather than offered.
// Published as surface so the acceptance specs compile against the real props; the bodies land in
// their own PRs' green phases.
export function AccountsScreen(_props: AccountsScreenProps) {
  return <div data-testid="accounts-screen" />;
}

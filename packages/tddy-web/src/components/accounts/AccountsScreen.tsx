/**
 * Accounts screen — every credential a daemon holds, grouped by provider.
 *
 * Presentational: it renders what it is given, matching the `HostsScreen` / `HostsAppPage` split.
 * `AccountsAppPage` owns the `ListAccounts` call and the four outcomes it can come back with, and
 * the `BeginLinkAccount` / `PollLinkAccount` pair behind the add-account control.
 *
 * **No row ever carries a secret.** `accounts.proto` has no field to put one in; `hasSecret` is the
 * single bit that tells a linked account from a stale row. The same holds through the link flow:
 * what a person sees of it is a short code and where to type it, and the credential it produces
 * never comes back up the wire.
 */

import { useState } from "react";
import { Button } from "../ui/button";

/**
 * `#keyring` 6/9's aggregate sync standing for one account, worst peer first. `null` means
 * nothing has synced the account yet — no peer configured to receive credentials, or none
 * admitted — and renders no badge at all, which is the common case for a daemon with no
 * `keyring.group_secret` configured.
 */
export type AccountSyncStatus = "synced" | "pending" | "undeliverable" | "conflict" | "refused";

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
  /** See [`AccountSyncStatus`]. */
  syncStatus: AccountSyncStatus | null;
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
 * What the daemon came back with. The four are held apart deliberately: an open-and-empty vault, no
 * vault yet, a vault that exists but is not unlocked on this daemon, and a read that failed are
 * different facts with different remedies — choose a passphrase, enter the passphrase, fix the
 * daemon. Rendering any two of them the same way would tell a person to re-link accounts they
 * already have.
 */
export type AccountsOutcome =
  | { kind: "listed"; providers: ProviderGroup[]; sessionAccount?: SessionAccountRef }
  | { kind: "uninitialized" }
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

export function AccountsScreen({
  outcome,
  linkAttempt,
  onRename,
  onRemove,
  onAddAccount,
}: AccountsScreenProps) {
  return (
    <div data-testid="accounts-screen">
      {renderOutcome(outcome, onRename, onRemove, onAddAccount)}
      {linkAttempt ? <LinkAttemptView attempt={linkAttempt} /> : null}
    </div>
  );
}

/**
 * Where an add-account attempt stands. The four states render different words on purpose: a
 * refusal at the provider, an expired code and a vault this session cannot open are not the same
 * thing to say to a person.
 */
function LinkAttemptView({ attempt }: { attempt: LinkAttempt }) {
  switch (attempt.kind) {
    case "awaiting":
      return (
        <div data-testid="accounts-link-awaiting" className="mt-4 text-sm space-y-1">
          <p>
            Enter this code at{" "}
            <a
              data-testid="accounts-link-verification-uri"
              className="underline"
              href={attempt.verificationUri}
              target="_blank"
              rel="noreferrer"
            >
              {attempt.verificationUri}
            </a>{" "}
            to add the {attempt.provider} account:
          </p>
          <p data-testid="accounts-link-user-code" className="font-mono text-lg">
            {attempt.userCode}
          </p>
        </div>
      );
    case "denied":
      return (
        <p data-testid="accounts-link-denied" className="mt-4 text-sm">
          The account was not added: authorization was refused at the provider.
        </p>
      );
    case "expired":
      return (
        <p data-testid="accounts-link-expired" className="mt-4 text-sm">
          The code expired before it was approved. Add the account again to get a new one.
        </p>
      );
    case "locked":
      return (
        <p data-testid="accounts-link-locked" className="mt-4 text-sm">
          The account was approved, but your credential vault is locked on this daemon, so it could
          not be stored. Unlock the vault with your passphrase and add the account again.
        </p>
      );
  }
}

function renderOutcome(
  outcome: AccountsOutcome,
  onRename: AccountsScreenProps["onRename"],
  onRemove: AccountsScreenProps["onRemove"],
  onAddAccount: AccountsScreenProps["onAddAccount"],
) {
  switch (outcome.kind) {
    case "uninitialized":
      return (
        <div data-testid="accounts-uninitialized" className="text-sm space-y-1">
          <p className="font-medium">You have no credential vault on this daemon yet.</p>
          <p className="text-muted-foreground">
            Linked accounts are kept in a vault sealed under a passphrase you choose. Choose one to
            create it: the dashboard asks for it in a prompt over every screen.
          </p>
          <PromptWhereabouts />
        </div>
      );
    case "locked":
      return (
        <div data-testid="accounts-locked" className="text-sm space-y-1">
          <p className="font-medium">Your credential vault is locked on this daemon.</p>
          <p className="text-muted-foreground">
            Unlock it with your passphrase to see your accounts; nothing in it is lost. The dashboard
            asks for the passphrase in a prompt over every screen.
          </p>
          <PromptWhereabouts />
        </div>
      );
    case "error":
      return (
        <p data-testid="accounts-error" className="text-sm text-destructive">
          {outcome.reason}
        </p>
      );
    case "listed":
      if (outcome.providers.length === 0) {
        return (
          <p data-testid="accounts-empty" className="text-muted-foreground text-sm">
            No accounts are linked on this daemon yet. An account appears here once you link it
            from the provider it belongs to.
          </p>
        );
      }
      return (
        <div className="space-y-6">
          {outcome.providers.map((group) => (
            <section key={group.provider} data-testid={`accounts-group-${group.provider}`}>
              <h2 className="text-sm font-semibold mb-2">{group.provider}</h2>
              <ul className="space-y-2">
                {group.accounts.map((account) => (
                  <AccountRowView
                    key={account.accountId}
                    provider={group.provider}
                    account={account}
                    isSessionAccount={
                      outcome.sessionAccount?.provider === group.provider &&
                      outcome.sessionAccount.accountId === account.accountId
                    }
                    onRename={onRename}
                    onRemove={onRemove}
                  />
                ))}
              </ul>
              <Button
                size="sm"
                variant="outline"
                className="mt-2"
                data-testid={`accounts-add-${group.provider}`}
                onClick={() => onAddAccount(group.provider)}
              >
                Add account
              </Button>
            </section>
          ))}
        </div>
      );
  }
}

/**
 * The passphrase is asked for by the app-wide `CredentialVaultPrompt`, which opens by itself while
 * the vault is locked or not created — this screen points at it rather than asking a second time.
 * Its "Not now" hides it until the page next loads, so that is the way back to it.
 */
function PromptWhereabouts() {
  return (
    <p className="text-muted-foreground">
      If you closed that prompt with “Not now”, reload the page to see it again.
    </p>
  );
}

interface AccountRowViewProps {
  provider: string;
  account: AccountRow;
  /** Whether this is the account the session was established with. */
  isSessionAccount: boolean;
  onRename: AccountsScreenProps["onRename"];
  onRemove: AccountsScreenProps["onRemove"];
}

function AccountRowView({
  provider,
  account,
  isSessionAccount,
  onRename,
  onRemove,
}: AccountRowViewProps) {
  const rowId = `accounts-row-${provider}-${account.accountId}`;
  const [draftLabel, setDraftLabel] = useState(account.label);
  // Removal is irreversible from this screen, so the first press only asks.
  const [confirmingRemoval, setConfirmingRemoval] = useState(false);

  return (
    <li data-testid={rowId} className="flex flex-wrap items-center gap-3 text-sm">
      <span data-testid={`${rowId}-label`} className="font-medium">
        {account.label}
      </span>
      <span data-testid={`${rowId}-subject`} className="text-muted-foreground">
        {account.subject}
      </span>
      {isSessionAccount ? (
        <span data-testid={`${rowId}-session`} className="text-xs text-emerald-700">
          Signed in with this account
        </span>
      ) : null}
      {account.hasSecret ? null : (
        <span className="text-amber-700 text-xs">no credential stored</span>
      )}
      <SyncStatusBadge rowId={rowId} status={account.syncStatus} />
      <form
        className="flex items-center gap-2"
        onSubmit={(event) => {
          event.preventDefault();
          onRename(provider, account.accountId, draftLabel);
        }}
      >
        <input
          data-testid={`${rowId}-rename-input`}
          aria-label={`New label for ${account.label}`}
          className="h-7 rounded-md border border-border bg-background px-2 text-sm"
          value={draftLabel}
          onChange={(event) => setDraftLabel(event.target.value)}
        />
        <Button type="submit" size="sm" variant="outline" data-testid={`${rowId}-rename-submit`}>
          Rename
        </Button>
      </form>
      {isSessionAccount ? null : confirmingRemoval ? (
        <span className="flex items-center gap-2">
          <span>Remove this account's credential?</span>
          <Button
            size="sm"
            variant="destructive"
            data-testid={`${rowId}-remove-confirm`}
            onClick={() => {
              setConfirmingRemoval(false);
              onRemove(provider, account.accountId);
            }}
          >
            Remove
          </Button>
          <Button size="sm" variant="ghost" onClick={() => setConfirmingRemoval(false)}>
            Cancel
          </Button>
        </span>
      ) : (
        <Button
          size="sm"
          variant="ghost"
          data-testid={`${rowId}-remove`}
          onClick={() => setConfirmingRemoval(true)}
        >
          Remove…
        </Button>
      )}
    </li>
  );
}

/** Text and color per [`AccountSyncStatus`] — worst (`refused`) reads the most alarming. */
const SYNC_STATUS_PRESENTATION: Record<AccountSyncStatus, { label: string; className: string }> = {
  synced: { label: "Synced", className: "text-emerald-700" },
  pending: { label: "Pending", className: "text-muted-foreground" },
  undeliverable: { label: "Undeliverable", className: "text-amber-700" },
  conflict: { label: "Conflict", className: "text-amber-700" },
  refused: { label: "Refused", className: "text-destructive" },
};

/**
 * `#keyring` 6/9's aggregate sync badge. Renders nothing at all for `null` — a daemon with no
 * `keyring.group_secret` configured, the common case — rather than an empty or zero-value badge.
 */
function SyncStatusBadge({ rowId, status }: { rowId: string; status: AccountSyncStatus | null }) {
  if (status === null) return null;
  const { label, className } = SYNC_STATUS_PRESENTATION[status];
  return (
    <span data-testid={`${rowId}-sync-status`} className={`text-xs ${className}`}>
      {label}
    </span>
  );
}

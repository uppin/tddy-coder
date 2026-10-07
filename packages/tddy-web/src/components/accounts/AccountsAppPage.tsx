/**
 * Data container for the Accounts screen: one `ListAccounts` call against the selected daemon, plus
 * the `BeginLinkAccount` / `PollLinkAccount` pair behind the add-account control.
 *
 * One RPC, deliberately — a vault's contents change only when somebody links, renames or removes an
 * account, and each of those is an action this screen already knows it took. A rename answers with
 * the account as it now stands and a removal with what remains, so neither re-reads.
 *
 * **Nothing here signs anybody in.** The link pair is on `AccountsService` rather than
 * `AuthService`, and neither response has a token field, so completing a link cannot replace the
 * session this page is already reading the vault with.
 */

import { useEffect, useRef, useState } from "react";
import { ConnectError } from "@connectrpc/connect";
import {
  AccountsService,
  LinkState,
  SyncStatus,
  type AccountSummary,
  type ListAccountsResponse,
  type ProviderAccounts,
} from "../../gen/accounts_pb";
import { useAuthContext } from "../../hooks/authProvider";
import { useDaemonClient } from "../../rpc/selectedDaemon";
import { AppShell } from "../shell/AppShell";
import {
  AccountsScreen,
  type AccountRow,
  type AccountSyncStatus,
  type AccountsOutcome,
  type LinkAttempt,
  type ProviderGroup,
} from "./AccountsScreen";

/** `SYNC_STATUS_UNSPECIFIED` maps to `null` — nothing has synced the account yet. */
function syncStatusFromRpc(status: SyncStatus): AccountSyncStatus | null {
  switch (status) {
    case SyncStatus.SYNCED:
      return "synced";
    case SyncStatus.PENDING:
      return "pending";
    case SyncStatus.UNDELIVERABLE:
      return "undeliverable";
    case SyncStatus.CONFLICT:
      return "conflict";
    case SyncStatus.REFUSED:
      return "refused";
    case SyncStatus.UNSPECIFIED:
    default:
      return null;
  }
}

function rowFromRpc(account: AccountSummary): AccountRow {
  return {
    accountId: account.accountId,
    label: account.label,
    subject: account.subject,
    updatedAtUnixSeconds: account.updatedAt,
    hasSecret: account.hasSecret,
    syncStatus: syncStatusFromRpc(account.syncStatus),
  };
}

function groupsFromRpc(providers: ProviderAccounts[]): ProviderGroup[] {
  return providers.map((group) => ({
    provider: group.provider,
    accounts: group.accounts.map(rowFromRpc),
  }));
}

/** A listing is one of three answers; the fourth outcome, an error, arrives as a rejection. */
function outcomeFromRpc(res: ListAccountsResponse): AccountsOutcome {
  if (res.vaultUninitialized) return { kind: "uninitialized" };
  if (res.vaultLocked) return { kind: "locked" };
  return {
    kind: "listed",
    providers: groupsFromRpc(res.providers),
    sessionAccount: res.sessionAccount && {
      provider: res.sessionAccount.provider,
      accountId: res.sessionAccount.accountId,
    },
  };
}

/** The daemon's reason, verbatim — without the transport's `[code]` prefix. */
function reasonOf(error: unknown): string {
  return ConnectError.from(error).rawMessage;
}

/** `providers` with one account's row replaced by `renamed`. */
function withRenamed(providers: ProviderGroup[], provider: string, renamed: AccountRow) {
  return providers.map((group) =>
    group.provider !== provider
      ? group
      : {
          ...group,
          accounts: group.accounts.map((account) =>
            account.accountId === renamed.accountId ? renamed : account,
          ),
        },
  );
}

export function AccountsAppPage({ onNavigate }: { onNavigate: (path: string) => void }) {
  const { sessionToken } = useAuthContext();
  const client = useDaemonClient(AccountsService);

  const [outcome, setOutcome] = useState<AccountsOutcome | null>(null);
  // A failed rename or removal is reported beside the list rather than replacing it: the list the
  // person was looking at is still what the vault holds.
  const [actionError, setActionError] = useState<string | null>(null);

  const [linkAttempt, setLinkAttempt] = useState<LinkAttempt | undefined>(undefined);
  // The pending poll timer, so leaving the page or starting another attempt stops it.
  const pollTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  useEffect(() => () => clearTimeout(pollTimer.current), []);

  // One read per visit; it re-fires only when the selected daemon or the session token changes,
  // and each of those genuinely invalidates the answer (see `HostsAppPage`).
  useEffect(() => {
    if (!client) return;
    let current = true;
    client
      .listAccounts({ sessionToken: sessionToken ?? "" })
      .then((res) => {
        if (!current) return;
        setOutcome(outcomeFromRpc(res));
      })
      .catch((e: unknown) => {
        if (!current) return;
        setOutcome({ kind: "error", reason: reasonOf(e) });
      });
    return () => {
      current = false;
    };
  }, [client, sessionToken]);

  const rename = (provider: string, accountId: string, label: string) => {
    if (!client) return;
    client
      .setAccountLabel({ sessionToken: sessionToken ?? "", provider, accountId, label })
      .then((res) => {
        const renamed = res.account;
        if (!renamed) return;
        setActionError(null);
        setOutcome((previous) =>
          previous?.kind === "listed"
            ? {
                ...previous,
                providers: withRenamed(previous.providers, provider, rowFromRpc(renamed)),
              }
            : previous,
        );
      })
      .catch((e: unknown) => setActionError(reasonOf(e)));
  };

  const reread = () => {
    if (!client) return;
    client
      .listAccounts({ sessionToken: sessionToken ?? "" })
      .then((res) => setOutcome(outcomeFromRpc(res)))
      .catch((e: unknown) => setActionError(reasonOf(e)));
  };

  // Every poll, the first included, waits the interval the daemon last named: GitHub's device
  // flow answers `slow_down` to a poll that comes sooner than that.
  const schedulePoll = (linkId: string, intervalSeconds: number) => {
    pollTimer.current = setTimeout(() => pollLink(linkId, intervalSeconds), intervalSeconds * 1000);
  };

  const pollLink = (linkId: string, intervalSeconds: number) => {
    if (!client) return;
    client
      .pollLinkAccount({ sessionToken: sessionToken ?? "", linkId })
      .then((res) => {
        switch (res.state) {
          case LinkState.LINK_PENDING: {
            const next = res.intervalSeconds > 0n ? Number(res.intervalSeconds) : intervalSeconds;
            schedulePoll(linkId, next);
            return;
          }
          case LinkState.LINK_LINKED:
            setLinkAttempt(undefined);
            reread();
            return;
          case LinkState.LINK_DENIED:
            setLinkAttempt({ kind: "denied" });
            return;
          case LinkState.LINK_EXPIRED:
            setLinkAttempt({ kind: "expired" });
            return;
          case LinkState.LINK_VAULT_LOCKED:
            setLinkAttempt({ kind: "locked" });
            return;
          default:
            setLinkAttempt(undefined);
            setActionError("the daemon answered a link poll with an unknown state");
        }
      })
      .catch((e: unknown) => {
        setLinkAttempt(undefined);
        setActionError(reasonOf(e));
      });
  };

  const addAccount = (provider: string) => {
    if (!client) return;
    clearTimeout(pollTimer.current);
    setActionError(null);
    client
      .beginLinkAccount({ sessionToken: sessionToken ?? "", provider })
      .then((res) => {
        setLinkAttempt({
          kind: "awaiting",
          provider,
          userCode: res.userCode,
          verificationUri: res.verificationUri,
        });
        schedulePoll(res.linkId, Number(res.intervalSeconds));
      })
      .catch((e: unknown) => setActionError(reasonOf(e)));
  };

  const remove = (provider: string, accountId: string) => {
    if (!client) return;
    client
      .removeAccount({ sessionToken: sessionToken ?? "", provider, accountId })
      .then((res) => {
        setActionError(null);
        // The response carries what remains, not who the session is — and removal never changes that.
        setOutcome((previous) => ({
          kind: "listed",
          providers: groupsFromRpc(res.providers),
          sessionAccount: previous?.kind === "listed" ? previous.sessionAccount : undefined,
        }));
      })
      .catch((e: unknown) => setActionError(reasonOf(e)));
  };

  return (
    <AppShell title="Accounts" onNavigate={onNavigate} dataTestId="accounts-app-page">
      {actionError ? (
        <p className="mb-3 text-sm text-destructive" data-testid="accounts-action-error">
          {actionError}
        </p>
      ) : null}
      {outcome ? (
        <AccountsScreen
          outcome={outcome}
          onRename={rename}
          onRemove={remove}
          linkAttempt={linkAttempt}
          onAddAccount={addAccount}
        />
      ) : null}
    </AppShell>
  );
}

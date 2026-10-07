/**
 * A daemon, from the Accounts screen's side, for the add-account flow.
 *
 * `aDaemonWherePollsAre(...)` names what each successive poll answers; the last answer repeats. It
 * also records what the page asked, so a spec can say "no poll was made yet" or "every request
 * carried the same session" without reaching into the backend.
 */

import { Code, ConnectError } from "@connectrpc/connect";
import { anInMemoryRpcBackend, type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { AuthService } from "../../../src/gen/auth_pb";
import {
  AccountsService,
  LinkState,
  type AccountSummary,
  type ProviderAccounts,
} from "../../../src/gen/accounts_pb";

export const LINK_UPDATED_AT = 1_726_700_000n;
export const LINK_USER_CODE = "WXYZ-1234";
export const LINK_VERIFICATION_URI = "https://github.com/login/device";
export const LINK_ID = "link-1";
/** What the provider asks between polls, in seconds. */
export const LINK_INTERVAL_SECONDS = 5;
export const LINK_INTERVAL_MS = LINK_INTERVAL_SECONDS * 1000;
/** Why a request the daemon could not answer failed; the page shows it verbatim. */
export const DAEMON_FAILURE_REASON = "the daemon is restarting";

/** A poll that is not answered at all, rather than answered with a state. */
export const POLL_FAILS = "poll-fails" as const;
export type PollAnswer = LinkState | typeof POLL_FAILS;

export interface DaemonOptions {
  /** `BeginLinkAccount` itself fails. */
  beginFails?: boolean;
  /** The interval, in seconds, the daemon names. Default {@link LINK_INTERVAL_SECONDS}. */
  intervalSeconds?: bigint;
}

export function anAccount(overrides: Partial<AccountSummary>): AccountSummary {
  return {
    provider: "github",
    accountId: "account-ada",
    label: "Ada at work",
    subject: "ada",
    updatedAt: LINK_UPDATED_AT,
    hasSecret: true,
    ...overrides,
  } as AccountSummary;
}

/** The account the session was established with — the one a person signed in as. */
export const ADA = anAccount({ accountId: "account-ada", label: "Ada at work", subject: "ada" });
/** A second account at the same provider, added by linking. */
export const GRACE = anAccount({ accountId: "account-grace", label: "Grace", subject: "grace" });
export const SESSION_IS_ADA = { provider: "github", accountId: "account-ada" };

export function githubHolding(accounts: AccountSummary[]): ProviderAccounts[] {
  return [{ provider: "github", accounts } as ProviderAccounts];
}

export interface LinkingDaemon {
  backend: InMemoryRpcBackend;
  /** The provider of every `BeginLinkAccount`, in order. */
  begins: string[];
  /** The link id of every `PollLinkAccount`, in order. */
  polls: string[];
  /** The session token every request carried, in order. */
  sessionTokensSeen: string[];
  /** Requests that establish a session — which linking must never make. */
  sessionEstablishingCalls: () => number;
}

/**
 * A daemon whose vault holds Ada, where successive polls answer `answers` (the last repeats).
 *
 * The listing grows Grace once a poll has answered LINKED, which is what a daemon does: the record
 * is in the vault by the time the poll says so.
 */
export function aDaemonWherePollsAre(
  answers: PollAnswer[],
  options: DaemonOptions = {},
): LinkingDaemon {
  const intervalSeconds = options.intervalSeconds ?? BigInt(LINK_INTERVAL_SECONDS);
  const begins: string[] = [];
  const polls: string[] = [];
  const sessionTokensSeen: string[] = [];
  let linked = false;

  const backend = anInMemoryRpcBackend().implement(AccountsService, {
    listAccounts: (request) => {
      sessionTokensSeen.push(request.sessionToken);
      return {
        providers: githubHolding(linked ? [ADA, GRACE] : [ADA]),
        vaultLocked: false,
        sessionAccount: SESSION_IS_ADA,
      };
    },
    beginLinkAccount: (request) => {
      sessionTokensSeen.push(request.sessionToken);
      begins.push(request.provider);
      if (options.beginFails) throw new ConnectError(DAEMON_FAILURE_REASON, Code.Unavailable);
      return {
        linkId: LINK_ID,
        userCode: LINK_USER_CODE,
        verificationUri: LINK_VERIFICATION_URI,
        expiresInSeconds: 900n,
        intervalSeconds,
      };
    },
    pollLinkAccount: (request) => {
      sessionTokensSeen.push(request.sessionToken);
      polls.push(request.linkId);
      const answer = answers[Math.min(polls.length, answers.length) - 1];
      if (answer === POLL_FAILS) throw new ConnectError(DAEMON_FAILURE_REASON, Code.Unavailable);
      linked = answer === LinkState.LINK_LINKED;
      return { state: answer, account: GRACE, intervalSeconds };
    },
  });

  return {
    backend,
    begins,
    polls,
    sessionTokensSeen,
    sessionEstablishingCalls: () =>
      backend.callsTo(AuthService.method.exchangeCode).length +
      backend.callsTo(AuthService.method.startDeviceLogin).length +
      backend.callsTo(AuthService.method.pollDeviceLogin).length,
  };
}

/**
 * The add-account flow: `BeginLinkAccount`, then `PollLinkAccount` once per interval until the
 * attempt ends.
 *
 * **Nothing here signs anybody in.** The link pair is on `AccountsService` rather than
 * `AuthService`, and neither response has a token field, so completing a link cannot replace the
 * session the page is reading the vault with.
 *
 * Every async continuation carries the attempt it belongs to and does nothing once that attempt is
 * no longer current — because the person pressed Add again, or because the page is gone. That one
 * check is what keeps a double click from running two polling chains and an unmounted page from
 * setting state.
 */

import { useEffect, useRef, useState } from "react";
import type { Client } from "@connectrpc/connect";
import {
  LinkState,
  type AccountsService,
  type PollLinkAccountResponse,
} from "../../gen/accounts_pb";
import type { LinkAttempt } from "./AccountsScreen";
import { reasonOf } from "./rpcReason";

/** A daemon answering an interval of 0 must not turn the poll into a tight loop. */
export const MIN_POLL_INTERVAL_SECONDS = 1;

/**
 * Consecutive failed polls after which the attempt is given up. A single dropped request must not
 * abandon a code that is still valid for minutes; a daemon that never answers must not be polled
 * for ever.
 */
export const MAX_CONSECUTIVE_POLL_FAILURES = 3;

/** The states that end an attempt without linking anything, each with its own words on screen. */
const ENDED_WITHOUT_LINKING: Partial<Record<LinkState, LinkAttempt>> = {
  [LinkState.LINK_DENIED]: { kind: "denied" },
  [LinkState.LINK_EXPIRED]: { kind: "expired" },
  [LinkState.LINK_VAULT_LOCKED]: { kind: "locked" },
};

function clampedInterval(seconds: bigint | number, fallback: number): number {
  const named = Number(seconds);
  return Math.max(MIN_POLL_INTERVAL_SECONDS, named > 0 ? named : fallback);
}

export interface LinkFlowOptions {
  client: Client<typeof AccountsService> | null;
  sessionToken: string;
  /** The attempt linked an account: the listing should be read again. */
  onLinked: () => void;
  /** A failure that is not one of the attempt's own end states; the daemon's reason. */
  onError: (reason: string | null) => void;
}

export function useLinkFlow({ client, sessionToken, onLinked, onError }: LinkFlowOptions) {
  const [linkAttempt, setLinkAttempt] = useState<LinkAttempt | undefined>(undefined);
  // Bumped when a new attempt starts and when the page goes; a continuation holding an older
  // number is stale.
  const currentAttempt = useRef(0);
  const beginInFlight = useRef(false);
  const pollTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);

  useEffect(
    () => () => {
      currentAttempt.current += 1;
      clearTimeout(pollTimer.current);
    },
    [],
  );

  const isStale = (attempt: number) => attempt !== currentAttempt.current;

  // Every poll, the first included, waits the interval the daemon last named: GitHub's device
  // flow answers `slow_down` to a poll that comes sooner than that.
  const schedulePoll = (attempt: number, linkId: string, interval: number, failures: number) => {
    pollTimer.current = setTimeout(
      () => pollLink(attempt, linkId, interval, failures),
      interval * 1000,
    );
  };

  const settle = (
    attempt: number,
    linkId: string,
    interval: number,
    res: PollLinkAccountResponse,
  ) => {
    if (res.state === LinkState.LINK_PENDING) {
      schedulePoll(attempt, linkId, clampedInterval(res.intervalSeconds, interval), 0);
      return;
    }
    if (res.state === LinkState.LINK_LINKED) {
      setLinkAttempt(undefined);
      onLinked();
      return;
    }
    const ended = ENDED_WITHOUT_LINKING[res.state];
    if (ended) {
      setLinkAttempt(ended);
      return;
    }
    setLinkAttempt(undefined);
    onError("the daemon answered a link poll with an unknown state");
  };

  const pollFailed = (
    attempt: number,
    linkId: string,
    interval: number,
    failures: number,
    error: unknown,
  ) => {
    if (failures + 1 < MAX_CONSECUTIVE_POLL_FAILURES) {
      schedulePoll(attempt, linkId, interval, failures + 1);
      return;
    }
    setLinkAttempt(undefined);
    onError(reasonOf(error));
  };

  const pollLink = (attempt: number, linkId: string, interval: number, failures: number) => {
    if (!client || isStale(attempt)) return;
    client.pollLinkAccount({ sessionToken, linkId }).then(
      (res) => {
        if (!isStale(attempt)) settle(attempt, linkId, interval, res);
      },
      (error: unknown) => {
        if (!isStale(attempt)) pollFailed(attempt, linkId, interval, failures, error);
      },
    );
  };

  const addAccount = (provider: string) => {
    if (!client || beginInFlight.current) return;
    beginInFlight.current = true;
    clearTimeout(pollTimer.current);
    currentAttempt.current += 1;
    const attempt = currentAttempt.current;
    onError(null);
    client
      .beginLinkAccount({ sessionToken, provider })
      .then(
        (res) => {
          if (isStale(attempt)) return;
          setLinkAttempt({
            kind: "awaiting",
            provider,
            userCode: res.userCode,
            verificationUri: res.verificationUri,
          });
          schedulePoll(attempt, res.linkId, clampedInterval(res.intervalSeconds, 0), 0);
        },
        (error: unknown) => {
          if (!isStale(attempt)) onError(reasonOf(error));
        },
      )
      .finally(() => {
        beginInFlight.current = false;
      });
  };

  return { linkAttempt, addAccount };
}

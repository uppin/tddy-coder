/**
 * The shape of a session the daemon mints, and what one device-flow poll's answer calls for — the
 * pure half of `useAuth`, with no React and no storage.
 */
import { DeviceLoginState, type VaultState } from "../gen/auth_pb";
import type { GitHubUser, PollDeviceLoginResponse } from "../gen/auth_pb";

/**
 * Where a device-flow sign-in (`StartDeviceLogin` / `PollDeviceLogin`) stands.
 *
 * `awaiting-approval` carries what the operator needs to approve the attempt at GitHub. `denied`
 * and `expired` are distinct because the operator's next move differs — a refusal was theirs, an
 * expiry was the clock's — though both end the attempt and are left by starting a fresh one.
 * `failed` is an attempt that ended on an error the daemon returned rather than on GitHub's answer.
 */
export type DeviceLogin =
  | { phase: "idle" }
  | { phase: "starting" }
  | { phase: "awaiting-approval"; userCode: string; verificationUri: string }
  | { phase: "denied" }
  | { phase: "expired" }
  | { phase: "failed"; error: string };

/** A session as the daemon mints it — by `ExchangeCode` or by an approved device login. */
export interface MintedSession {
  sessionToken: string;
  refreshToken: string;
  vaultUnlockKey: string;
  vaultState: VaultState;
  user?: GitHubUser;
}

/** A minted session with every part present — the only kind the page takes up. */
export interface WholeSession {
  sessionToken: string;
  refreshToken: string;
  /** The unlock key for this lineage's credential-vault slot — `""` when the daemon keeps none. */
  vaultUnlockKey: string;
  /** Where the operator's credential vault stands after this sign-in. */
  vaultState: VaultState;
  user: GitHubUser;
}

export type SessionCheck = { whole: WholeSession } | { missing: string[] };

/**
 * Whether `minted` is a whole session, or which of its parts the daemon left out. proto3 leaves a
 * field the daemon never set as an absent message or an empty string; either is a missing part,
 * never one to fill in with a default.
 */
export function checkWholeSession({
  sessionToken,
  refreshToken,
  vaultUnlockKey,
  vaultState,
  user,
}: MintedSession): SessionCheck {
  const missing = [
    ...(user === undefined ? ["user"] : []),
    ...(sessionToken === "" ? ["session token"] : []),
    ...(refreshToken === "" ? ["refresh token"] : []),
  ];
  if (user === undefined || missing.length > 0) return { missing };
  // An empty unlock key is not a missing part: a stub login, or a daemon with no vault, keeps none.
  return { whole: { sessionToken, refreshToken, vaultUnlockKey, vaultState, user } };
}

/** The error for a flow that `completed` without the session parts named in `missing`. */
export function noWholeSessionMessage(completed: string, missing: string[]): string {
  return `${completed} without a whole session (no ${missing.join(", no ")})`;
}

/** Milliseconds per second: the daemon names poll intervals in seconds, `setTimeout` takes ms. */
export const MS_PER_SECOND = 1000;

/**
 * `seconds` — an interval the daemon named — as a delay between polls, or `null` when it is not a
 * positive interval. That is a protocol error, never a reason to poll with no delay.
 */
export function intervalMsOf(seconds: bigint): number | null {
  const n = Number(seconds);
  return n > 0 ? n * MS_PER_SECOND : null;
}

/**
 * What one poll's answer tells a device-flow attempt to do next: poll again after `afterMs` (every
 * later poll keeps that delay), end on `deviceLogin`, or take up the whole session it carries.
 */
export type DevicePollStep =
  | { next: "poll"; afterMs: number }
  | { next: "settle"; deviceLogin: DeviceLogin }
  | { next: "adopt"; session: WholeSession };

export function deviceLoginFailed(error: string): DevicePollStep {
  return { next: "settle", deviceLogin: { phase: "failed", error } };
}

/** The step `res` calls for, for an attempt currently polling every `intervalMs`. */
export function devicePollStep(res: PollDeviceLoginResponse, intervalMs: number): DevicePollStep {
  switch (res.state) {
    case DeviceLoginState.PENDING:
      return { next: "poll", afterMs: intervalMs };
    case DeviceLoginState.SLOW_DOWN: {
      const widenedMs = intervalMsOf(res.intervalSeconds);
      if (widenedMs === null) {
        return deviceLoginFailed("The daemon asked to slow down device sign-in without naming an interval");
      }
      return { next: "poll", afterMs: widenedMs };
    }
    case DeviceLoginState.COMPLETE: {
      const session = checkWholeSession(res);
      if ("missing" in session) {
        return deviceLoginFailed(noWholeSessionMessage("The daemon completed device sign-in", session.missing));
      }
      return { next: "adopt", session: session.whole };
    }
    case DeviceLoginState.DENIED:
      return { next: "settle", deviceLogin: { phase: "denied" } };
    case DeviceLoginState.EXPIRED:
      return { next: "settle", deviceLogin: { phase: "expired" } };
    default:
      return deviceLoginFailed(`Unrecognised device sign-in state ${res.state}`);
  }
}

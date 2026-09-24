/**
 * In-memory `auth.AuthService` backend for the **device-flow** sign-in
 * (`StartDeviceLogin` / `PollDeviceLogin`), the flow a deployment with a public `client_id` and no
 * client secret serves in place of the redirect flow.
 *
 * The backend is scripted: each `StartDeviceLogin` hands out the next device-code grant, and each
 * `PollDeviceLogin` answers with the next scripted state. The last scripted poll answer repeats, so
 * "pending forever" is a one-element script. Every request is recorded by the testkit, so a test
 * can count polls and read the device code each one presented.
 *
 * `GetAuthStatus` recognises the access token a completed login minted, so a client that validates
 * its new session finds it authenticated as the approved user.
 *
 * Proto: packages/tddy-service/proto/auth.proto
 */

import { anInMemoryRpcBackend, type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { create } from "@bufbuild/protobuf";
import {
  AuthService,
  DeviceLoginState,
  PollDeviceLoginResponseSchema,
  StartDeviceLoginResponseSchema,
  type GitHubUser,
  type PollDeviceLoginResponse,
  type StartDeviceLoginResponse,
} from "../../../src/gen/auth_pb";
import { CURRENT_ACCESS_TOKEN, VALID_REFRESH_TOKEN } from "./durableSessionBackend";
import { aGitHubUser } from "./responses";

export { ACCESS_TOKEN_KEY, REFRESH_TOKEN_KEY } from "./durableSessionBackend";

/** GitHub's device verification page — where the operator types the user code. */
export const GITHUB_DEVICE_VERIFICATION_URI = "https://github.com/login/device";

/** The access token a completed device login mints (a real, far-future `v1.` token). */
export const DEVICE_LOGIN_ACCESS_TOKEN = CURRENT_ACCESS_TOKEN;
/** The refresh token a completed device login mints alongside the access token. */
export const DEVICE_LOGIN_REFRESH_TOKEN = VALID_REFRESH_TOKEN;

/** The GitHub account that approves the device code. */
export function theApprovingOperator(): GitHubUser {
  return aGitHubUser({ login: "octocat", name: "The Octocat", id: BigInt(583231) });
}

// ---------------------------------------------------------------------------
// Grants — what StartDeviceLogin hands out
// ---------------------------------------------------------------------------

/** A device-code grant as `StartDeviceLogin` returns it. Override only what the test is about. */
export function aDeviceCodeGrant(
  overrides: Partial<{
    deviceCode: string;
    userCode: string;
    verificationUri: string;
    expiresInSeconds: number;
    intervalSeconds: number;
  }> = {},
): StartDeviceLoginResponse {
  const grant = {
    deviceCode: "3584d83530557fdd1f46af8289938c8ef79f9dc5",
    userCode: "WDJB-MJHT",
    verificationUri: GITHUB_DEVICE_VERIFICATION_URI,
    expiresInSeconds: 900,
    intervalSeconds: 5,
    ...overrides,
  };
  return create(StartDeviceLoginResponseSchema, {
    deviceCode: grant.deviceCode,
    userCode: grant.userCode,
    verificationUri: grant.verificationUri,
    expiresInSeconds: BigInt(grant.expiresInSeconds),
    intervalSeconds: BigInt(grant.intervalSeconds),
  });
}

// ---------------------------------------------------------------------------
// Poll answers — what PollDeviceLogin says about an attempt
// ---------------------------------------------------------------------------

/** Not approved yet. */
export function aPendingPoll(): PollDeviceLoginResponse {
  return create(PollDeviceLoginResponseSchema, { state: DeviceLoginState.PENDING });
}

/** Polled too fast — adopt `intervalSeconds`. */
export function aSlowDownPoll(intervalSeconds: number): PollDeviceLoginResponse {
  return create(PollDeviceLoginResponseSchema, {
    state: DeviceLoginState.SLOW_DOWN,
    intervalSeconds: BigInt(intervalSeconds),
  });
}

/** The operator refused the request at GitHub. */
export function aDeniedPoll(): PollDeviceLoginResponse {
  return create(PollDeviceLoginResponseSchema, { state: DeviceLoginState.DENIED });
}

/** The codes outlived their window. */
export function anExpiredPoll(): PollDeviceLoginResponse {
  return create(PollDeviceLoginResponseSchema, { state: DeviceLoginState.EXPIRED });
}

/** Approved — the same session triple `ExchangeCode` returns. */
export function aCompletedPoll(): PollDeviceLoginResponse {
  return create(PollDeviceLoginResponseSchema, {
    state: DeviceLoginState.COMPLETE,
    sessionToken: DEVICE_LOGIN_ACCESS_TOKEN,
    refreshToken: DEVICE_LOGIN_REFRESH_TOKEN,
    user: theApprovingOperator(),
  });
}

/** One of the three parts of the session a completed device login carries. */
export type CompletedSessionPart = "user" | "sessionToken" | "refreshToken";

/**
 * Approved, but without `missing` — the user absent, or a token empty, as proto3 leaves a field the
 * daemon never set. Every other part is exactly what {@link aCompletedPoll} carries.
 */
export function aCompletedPollMissing(missing: CompletedSessionPart): PollDeviceLoginResponse {
  const absent: Record<CompletedSessionPart, Partial<PollDeviceLoginResponse>> = {
    user: { user: undefined },
    sessionToken: { sessionToken: "" },
    refreshToken: { refreshToken: "" },
  };
  return create(PollDeviceLoginResponseSchema, {
    state: DeviceLoginState.COMPLETE,
    sessionToken: DEVICE_LOGIN_ACCESS_TOKEN,
    refreshToken: DEVICE_LOGIN_REFRESH_TOKEN,
    user: theApprovingOperator(),
    ...absent[missing],
  });
}

// ---------------------------------------------------------------------------
// Backend
// ---------------------------------------------------------------------------

export interface DeviceLoginScript {
  /** Grants handed out by successive `StartDeviceLogin` calls; the last one repeats. */
  grants?: StartDeviceLoginResponse[];
  /** Answers to successive `PollDeviceLogin` calls; the last one repeats. */
  polls: PollDeviceLoginResponse[];
}

/** The `index`th scripted item, repeating the last one once the script runs out. */
function scripted<T>(items: T[], index: number): T {
  return items[Math.min(index, items.length - 1)];
}

/** An `auth.AuthService` backend that walks a device-flow attempt through `script`. */
export function aDeviceLoginBackend(script: DeviceLoginScript): InMemoryRpcBackend {
  const grants = script.grants ?? [aDeviceCodeGrant()];
  let starts = 0;
  let polls = 0;
  return anInMemoryRpcBackend().implement(AuthService, {
    startDeviceLogin: async () => scripted(grants, starts++),
    pollDeviceLogin: async () => scripted(script.polls, polls++),
    getAuthStatus: async (req) =>
      req.sessionToken === DEVICE_LOGIN_ACCESS_TOKEN
        ? { authenticated: true, user: theApprovingOperator() }
        : { authenticated: false, user: undefined },
  });
}

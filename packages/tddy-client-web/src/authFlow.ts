/** The GitHub sign-in flows a daemon declares in `GET /api/config` (`auth_flow`) — what the sign-in screen is told, never guesses. */

/** The GitHub sign-in flows a daemon can serve. */
export type AuthFlow = "redirect" | "device";

/**
 * What a daemon declared about its GitHub sign-in.
 *
 * - `"redirect"` — `GetAuthUrl` / `ExchangeCode`, a deployment holding a client secret.
 * - `"device"` — `StartDeviceLogin` / `PollDeviceLogin`, a public client id and no secret.
 * - `"none"` — no `auth_flow` at all: the daemon serves no GitHub sign-in. Never read as a flow.
 * - `{ unrecognised }` — a value this page does not know, kept so the screen can name it rather
 *   than guess a flow the daemon may not serve.
 */
export type AuthFlowDeclaration = AuthFlow | "none" | { unrecognised: string };

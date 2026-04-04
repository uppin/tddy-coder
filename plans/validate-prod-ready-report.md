# Validate prod-ready — terminal reconnect overlay

**Scope:** `packages/tddy-web/src/components/connection/terminalPresentation.ts`, `packages/tddy-web/src/routing/appRoutes.ts`, `packages/tddy-web/src/components/ConnectionScreen.tsx` (integration context).  
**Date:** 2026-04-04

## Executive summary

URL construction and parsing for `/terminal/:sessionId` are sound: session segments are encoded for navigation and safely decoded with rejection of multi-segment or malformed paths. `ConnectionScreen` implements deep-link attach via `connectSession` with `resumeSession` fallback, cancellation via a sequence ref, and a clear UX path when the session is unknown. The main production gap is that **`terminalPresentation.ts` is not imported or used by the live app**—only by unit/acceptance tests—so presentation rules (overlay vs full, push vs no-push on reconnect) are **not enforced in the UI**. Logging in `terminalPresentation` uses `console.info` on attach transitions; if wired to frequent state updates, that would be noisy for production. The shell’s `usePathname` in `index.tsx` does not observe `pushState` from `ConnectionScreen.navigatePath`, so **App-level `path` can lag the real URL** until `popstate`; today this is mostly harmless because routing still chooses `ConnectionScreen` for non-`/worktrees` paths, but it is a footgun for future nav highlighting or guards.

## Checklist

| Area | Status | Notes |
|------|--------|--------|
| Error handling (RPC, deep link) | **Partial** | User-facing errors for start/connect/resume/delete; deep link sets error on final failure. `listSessions` failure is silent (empty list + hydrated). |
| Logging (`console.debug` / `console.info`) | **Partial** | `ConnectionScreen` uses `console.debug` for tools/agents load (reasonable). `terminalPresentation` uses `info` for attach transitions—too high if called often; `debug` for reconcile/placement is OK. |
| Configuration | **Pass** | LiveKit/presence from `/api/config`; RPC base `${origin}/rpc`—consistent with app. |
| Security — XSS | **Pass** | Errors rendered as React text nodes (`{error}`), not `dangerouslySetInnerHTML`. |
| Security — path injection | **Pass** | `terminalPathForSessionId` uses `encodeURIComponent`; `parseTerminalSessionIdFromPathname` rejects extra `/` segments and bad decode. |
| Security — session id in URLs | **Partial** | By design for deep links; appears in history, shareable URL, and server/access logs. Not secret, but privacy-sensitive if ids are ever treated as opaque. |
| Performance — hot paths | **Pass** | Pure helpers in `terminalPresentation` are cheap; no evidence of heavy work per frame. Polling intervals are bounded (2s/5s). |
| Performance — unnecessary work | **Partial** | `terminalDeepLinkSessionPath` logs on every call (currently thin wrapper). If called in a tight loop, avoid logging. |
| Integration — overlay PRD | **Fail** | `terminalPresentation` not wired into `ConnectionScreen` / terminal chrome; behavior exists only in tests. |

## Findings (by severity)

### High

1. **`terminalPresentation.ts` not integrated into runtime UI**  
   Grep shows imports only from `terminalPresentation.test.ts` and `ConnectionScreen.test.tsx`. `ConnectionScreen` does not call `attachKindForSessionControl`, `nextPresentationFromAttach`, or overlay reconciliation. Production behavior for “reconnect → overlay without route push” is not implemented in the component tree.

### Medium

2. **`console.info` in presentation helpers** (`terminalPresentation.ts`)  
   `nextPresentationFromAttach` and several transition helpers log at **info** level. If these become part of render or high-frequency handlers, logs will flood production consoles and may expose session flow details to anyone with devtools open.

3. **Silent failure on `listSessions`** (`ConnectionScreen.tsx`)  
   On catch, sessions are cleared and hydration completes with no `setError` or user-visible message—users may see an empty list after a transient RPC failure.

4. **Dual pathname state** (`index.tsx` + `ConnectionScreen`)  
   `ConnectionScreen.navigatePath` updates the URL and local `routePath` but does not update the parent `usePathname` state. Back/forward still work via `popstate`, but App `path` can disagree with `window.location.pathname` after in-app terminal navigation.

### Low

5. **`terminalDeepLinkSessionPath` logs encoded path**  
   `console.debug` includes the resolved path (derived from session id). Acceptable for debug, but confirms session id material in logs when devtools are open.

6. **Deep link: empty `catch` on first `connectSession`**  
   The outer `catch` intentionally tries `resumeSession`; only the inner failure surfaces. Correct behavior, but failures are opaque until the second call fails—acceptable for UX, slightly harder to diagnose without structured logging.

## Recommendations

1. **Integrate or defer:** Either wire `terminalPresentation` state into `ConnectionScreen` (and terminal chrome) so reconnect vs new attach matches the PRD, or treat the module as a spec-only stub and document it as FIXME until integrated—avoid shipping “prod-ready” claims for overlay behavior until then.

2. **Logging:** Prefer `console.debug` for all attach/presentation transitions in production builds, or gate behind an explicit `debugTerminalPresentation` flag (no silent fallbacks for behavior—only for verbosity).

3. **Sessions list errors:** On `listSessions` failure, set a non-blocking toast or inline warning, or preserve `sessions` and set `error`—avoid silent empty state when the failure is distinguishable from “no sessions.”

4. **Single source of truth for URL:** Consider having `ConnectionScreen` call `onNavigate` (or a dedicated `replacePath`) when updating terminal routes so `App`’s `path` stays aligned with `window.location`, **or** read `window.location.pathname` where needed instead of stale React state.

5. **Session IDs in URLs:** Document for operators/users that deep links are shareable and appear in logs; if stricter privacy is required later, use opaque tokens server-side (out of scope for this review).

---

*Validation method: static review of listed files, repo grep for `navigatePath`, `resumeSession`, `terminalPath`, and cross-reference with `packages/tddy-web/src/index.tsx` routing.*

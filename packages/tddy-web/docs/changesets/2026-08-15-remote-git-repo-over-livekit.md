# 2026-08-15 — remote-git-repo-over-livekit

**Type:** Fix

`token.TokenService` calls now carry a session token. No call-site change was needed: `runUnaryCall` creates the request message before interceptors run, so adding the proto field enrolled the service in the existing `authGateInterceptor`, which injects a request-time-fresh token — reading one from `useAuthContext` instead would be staler and would force an `AuthProvider` into ~50 specs. Three Cypress specs now assert the credential on the wire. (tddy-web)

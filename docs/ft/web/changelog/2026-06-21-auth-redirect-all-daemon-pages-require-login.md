# 2026-06-21 — Auth redirect: all daemon pages require login

- All daemon-mode pages now gate on auth at the `App` level; unauthenticated visitors see a login screen with "Sign in with GitHub"
- `login(returnTo?)` saves the current hash path to `sessionStorage`; `AuthCallback` redirects to `/#<returnTo>` after OAuth completes, returning users to the page they were trying to access

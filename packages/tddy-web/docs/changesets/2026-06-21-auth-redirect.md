# 2026-06-21 — **Auth redirect

**Type:** Feature

daemon pages require login with return-to** — `App` gates all daemon-mode routes on `useAuth()`; login passes current hash path as `returnTo`; `AuthCallback` redirects to `/#<returnTo>` after OAuth. (tddy-web)

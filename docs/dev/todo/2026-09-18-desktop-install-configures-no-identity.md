# 2026-09-18 — A desktop install configures no identity, so it has no sessions

**Category:** Defect / deferred scope — **narrowed 2026-09-24** by `#keyring` 2/9 (#509)
**Owner:** the developer ("I'll configure and test production myself") — **delete this entry when
they confirm** a fresh install signs in.

Source: `./install --desktop` ([2026-09-18-install-desktop.md](../changesets/2026-09-18-install-desktop.md)).
Features [tddy-desktop-tauri.md](../../ft/desktop/tddy-desktop-tauri.md),
[daemon-settings.md](../../ft/daemon/daemon-settings.md).

## What remains

1. **Render the OAuth App's public `client_id` into `packages/tddy-desktop/desktop.yaml.production`**
   (`github: { client_id: … }`, **no** `client_secret`), and rewrite the template's barrier notes
   (`desktop.yaml.production`, the `github:` / `users:` comments), which still tell an operator to add
   a `client_secret` and a `users:` row.
2. **Verify a fresh `./install --desktop` reaches a signed-in dashboard with no file edited by hand**
   — open the app, approve the device code on github.com, see sessions. While there, confirm against
   the live API that the OAuth App's device-flow token arrives with no expiring `refresh_token`
   (#509 risk V8): a GitHub App's would need a secret to refresh.

Both need the registered OAuth App, which is the developer's.

## What is no longer in the way

The entry originally recorded three requirements that all had to be met together. The code side of
each is done:

- **`livekit.api_secret` as the only signer source** — gone with `#keyring` 1/9 (#508): a daemon with
  no `livekit:` block signs session tokens with its own per-daemon Ed25519 key.
- **`github:` needing `client_id` + `client_secret`** — gone with `#keyring` 2/9 (#509): a
  `client_id` alone registers `auth.AuthService` with the **GitHub device flow**
  (`StartDeviceLogin` / `PollDeviceLogin`), and the dashboard signs in by device code.
- **`users: []` refusing every login** — gone with #509: a desktop's **first** sign-in from its own
  window enrols that GitHub login against the OS user the app runs as and persists the row to
  `~/.tddy/desktop.yaml`. `os_user_for_github` is unchanged; a second, different login is refused.

So the only thing between a fresh install and a signed-in dashboard is the rendered `client_id`
above — the file ships with `github:` unset, which a running daemon reports as "no sign-in
configured".

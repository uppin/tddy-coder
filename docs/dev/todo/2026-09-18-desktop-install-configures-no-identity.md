# 2026-09-18 — A desktop install configures no identity, so it has no sessions

**Category:** Defect / deferred scope — **narrowed 2026-09-24** by `#keyring` 2/9 (#509) to a verification
**Owner:** the developer ("I'll configure and test production myself") — **delete this entry when
they confirm** a fresh install signs in.

Source: `./install --desktop` ([2026-09-18-install-desktop.md](../changesets/2026-09-18-install-desktop.md)).
Features [tddy-desktop-tauri.md](../../ft/desktop/tddy-desktop-tauri.md),
[daemon-settings.md](../../ft/daemon/daemon-settings.md).

## What remains

**Verify a fresh `./install --desktop` reaches a signed-in dashboard with no file edited by hand**
— open the app, approve the device code on github.com, see sessions. While there, confirm against the
live API that the OAuth App's device-flow token arrives with no expiring `refresh_token` (#509 risk
V8): a GitHub App's would need a secret to refresh.

The client id itself is done: `desktop.yaml.production` renders `github: { client_id:
"Ov23lioH6CfiaZR8ESr5" }` with no secret (2026-09-24, #509). This check needs the real application
and a GitHub approval, so it is the developer's.

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

So nothing is known to stand between a fresh install and a signed-in dashboard; the check above is
what confirms it.

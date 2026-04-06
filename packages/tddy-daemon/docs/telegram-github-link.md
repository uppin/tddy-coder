# Telegram ↔ GitHub identity (`telegram_github_link`)

Technical reference for **`tddy_daemon::telegram_github_link`**.

## Purpose

Binds a **Telegram user id** to a **GitHub login** using the same **`daemon.yaml`** **`users:`** mapping (**`github_user`** → **`os_user`**) as web clients after OAuth. Supports automated tests with **`StubGitHubProvider`** without live GitHub or Telegram.

## Components

| Item | Role |
|------|------|
| **`TelegramOAuthStateSigner`** | Builds OAuth **`state`** strings: **`v1.`** + URL-safe base64 of **`[version][telegram_user_id LE]`** + **HMAC-SHA256** over that payload. Verification uses constant-time comparison of the MAC. |
| **`TelegramGithubMappingStore`** | Persists **`telegram_user_id` → `github_login`** as JSON on disk. Writes use a temporary file in the same directory and **`rename`** for atomic replace. |
| **`resolved_os_user_for_telegram_workflow`** | Looks up **`github_login`** for a Telegram user, then **`DaemonConfig::os_user_for_github`**. |
| **`complete_telegram_link_via_stub_exchange`** | Calls **`GitHubOAuthProvider::authorize_url`** then **`exchange_code`** on a **`StubGitHubProvider`**, then **`put`** on the store. |

## Harness integration

**`TelegramSessionControlHarness::with_telegram_github_link`** stores a mapping file path. When present, **`handle_start_workflow`** opens the store and requires **`get_github_login(user_id)`** before creating a session directory; failure returns an error that references linking GitHub (including **`/link-github`** in the message text).

## Security and logging

- OAuth **`state`** carries no secrets in plaintext beyond the bound user id; HMAC uses a server-held key (**`TelegramOAuthStateSigner::new`**).
- Structured logging uses **`log`** targets under **`tddy_daemon::telegram_github_link`** and **`tddy_daemon::telegram_session_control`**; authorization codes and tokens are not logged.

## Tests

- Integration: **`packages/tddy-daemon/tests/telegram_github_link.rs`**
- Unit: **`#[cfg(test)]`** in **`packages/tddy-daemon/src/telegram_github_link.rs`**

## Related

- Feature: **[telegram-session-control.md](../../../docs/ft/daemon/telegram-session-control.md)**
- Config mapping: **`DaemonConfig::users`** / **`os_user_for_github`** (see **[connection-service.md](./connection-service.md)**)

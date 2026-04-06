# Changeset: Telegram ↔ GitHub user linking (daemon library)

**Date**: 2026-04-06  
**Status**: Complete  
**Type**: Feature

## Affected packages

- **tddy-daemon**
- **docs** (`docs/ft/daemon/`, `docs/dev/`)

## Related feature documentation

- [telegram-session-control.md](../../../docs/ft/daemon/telegram-session-control.md)
- [daemon/changelog.md](../../../docs/ft/daemon/changelog.md)

## Summary

The daemon library exposes **`telegram_github_link`**: OAuth **`state`** signing (HMAC-SHA256), a durable JSON mapping store, OS-user resolution via **`DaemonConfig::users`**, and a stub OAuth completion helper. **`TelegramSessionControlHarness`** supports an optional mapping file path so **`handle_start_workflow`** fails for unlinked Telegram users with an explicit error (no session directory creation).

## Scope

- [x] Feature docs (`docs/ft/daemon/`) describe behavior in present-state language
- [x] Package technical doc **`telegram-github-link.md`**
- [x] **`packages/tddy-daemon/docs/changesets.md`** and **`docs/dev/changesets.md`** entries

## Technical state (reference)

- **`TelegramOAuthStateSigner`**: **`v1.`** + base64url payload + MAC; constant-time MAC verify
- **`TelegramGithubMappingStore`**: JSON file; atomic write via temp + rename
- **`complete_telegram_link_via_stub_exchange`**: **`authorize_url`** + **`exchange_code`** on **`StubGitHubProvider`**, then **`put`**

## Acceptance tests (implementation)

- **`packages/tddy-daemon/tests/telegram_github_link.rs`**
- Unit tests in **`src/telegram_github_link.rs`**

## Production follow-ups (outside this doc transfer)

- Wire **`TelegramWorkflowSpawn`** OS user from mapping
- HTTP OAuth callback validates **`TelegramOAuthState`**
- Telegram bot commands start the live OAuth URL flow
- **`daemon.yaml`** mapping file path configuration

## References

- [telegram-github-link.md](../../../packages/tddy-daemon/docs/telegram-github-link.md)
- [evaluation report](../../../plans/evaluation-report.md) (workspace plans)

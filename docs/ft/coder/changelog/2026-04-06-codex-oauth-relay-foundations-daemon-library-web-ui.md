# 2026-04-06 — Codex OAuth relay foundations (daemon library; web UI)

- **tddy-daemon**: **`codex_oauth_relay`** validates authorize URLs and parses OAuth callbacks for future **`BROWSER`** capture and Codex listener relay (**`tddy-integration-tests`**: **`codex_oauth_web_relay_acceptance`**).
- **tddy-web**: **`CodexOAuthDialog`** for authorize URL display (iframe vs embedding-blocked link). Product doc: **[codex-oauth-web-relay.md](../../web/codex-oauth-web-relay.md)**; daemon product doc: **[codex-oauth-relay.md](../../daemon/codex-oauth-relay.md)**. Cross-package: **[docs/dev/changesets/](../../../dev/changesets/)**.

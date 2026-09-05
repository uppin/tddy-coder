# 2026-03-29 — Web daemon: stub OAuth when stub codes are set

- **`tddy-coder`**: **`build_auth_service_entry`** treats non-empty **`--github-stub-codes`** (after trim) as stub auth mode alongside **`--github-stub`**, wiring **`StubGitHubProvider`** and optional code→user mappings for automated browser sign-in (e.g. Cypress **`app-connect`** flows).
- **Operational note**: Production-style launches must omit stray **`--github-stub-codes`** values unless stub authentication is deliberate.
- **Feature / cross-package**: [web-terminal.md](../../web/web-terminal.md) (connection flows); [web changelog](../../web/changelog/) **2026-03-29**.

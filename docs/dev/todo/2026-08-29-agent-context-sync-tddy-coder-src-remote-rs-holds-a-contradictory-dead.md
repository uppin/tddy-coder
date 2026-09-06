# 2026-08-29 — Agent context sync — `tddy-coder/src/remote.rs` holds a contradictory dead duplicate

**Category:** Future enhancement
**Source:** agent-context-sync changeset, 2026-08-29

`REMOTE_APPENDIX`, `RemoteContextDir`, `copy_dir_recursive` and `make_readonly_recursive` have no
caller in any `src/` in the workspace — only `tddy-coder`'s own `remote_bootstrap_acceptance` and
`remote_mode_acceptance`. Its docs now say plainly that it is legacy and which of its claims are
false, so it no longer teaches a reader the opposite of shipped behaviour, but the duplication
remains. Deleting it together with those two acceptance tests is the right answer and is an **ASK**
per AGENTS.md. `tddy-coder` does not depend on `tddy-sandbox`, so delegating to
`MANAGED_CODEBASE_PREAMBLE` instead would mean adding that dependency.

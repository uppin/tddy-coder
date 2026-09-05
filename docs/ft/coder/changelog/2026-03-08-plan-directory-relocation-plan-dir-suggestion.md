# 2026-03-08 — Plan Directory Relocation (plan_dir_suggestion)

- **Agent-decided location**: When the plan agent returns `plan_dir_suggestion` in discovery, the workflow relocates the session directory from staging (output_dir) to the suggested path relative to the git root (e.g. `docs/dev/1-WIP/2026-03-08-feature/`).
- **Exit output**: On successful exit, tddy-coder prints the session directory path (plan, acceptance-tests, red, green goals and full workflow).
- **Resume**: Full workflow resume requires `--session-dir`; automatic discovery removed.
- **Validation**: Invalid suggestions (absolute paths, `..`, empty) fall back to staging location. Cross-device moves use copy-then-delete when rename fails.

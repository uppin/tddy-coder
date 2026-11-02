# 2026-11-02 — tddy-tools build --hermetic

**Type:** Feature

`build` resolves tools against `<tddyhome>/tools`, jails `sandbox: true` actions (all with `--hermetic`) under `<tddyhome>/build-jails` via `DaemonActionSandbox`; `--hermetic` over `TDDY_SOCKET` is refused. Cross-package: [agent-tools-build-tools](../../../../docs/dev/changesets/2026-11-02-agent-tools-build-tools.md).

# 2026-11-02 — Declared tools: a jail runs a binary from a YAML tool definition

`#agent-tools` 1/12 ([#790](https://github.com/uppin/tddy-coder/pull/790)).

- **A tool is a YAML definition** — `id`, `summary`, `binary { command, env }`, `deps`, `reads` —
  built in (`rg`) or overridden whole from `<tddyhome>/tools/*.yaml`. Bad files, duplicate ids,
  unknown deps and dep cycles are refused naming the files and ids.
- **`Grep` works in the workspace tool jail on macOS.** The jail grants the resolved `rg` file (not its
  directory) and puts it on a jail-local `PATH` entry; no sibling binary becomes executable.
- **A host without `rg` refuses the jail at start**, naming the tool and what was searched, instead of
  failing every `Grep` call. `TDDY_RG_BINARY` or an override file points it elsewhere.

See [Sandbox builder § Declared tools](../sandbox-builder.md#declared-tools).

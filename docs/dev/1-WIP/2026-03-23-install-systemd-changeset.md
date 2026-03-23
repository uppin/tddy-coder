# Changeset: `./install --systemd`

**Date**: 2026-03-23  
**PRD**: [docs/ft/daemon/1-WIP/PRD-2026-03-23-install-systemd.md](../../ft/daemon/1-WIP/PRD-2026-03-23-install-systemd.md)  
**Plan**: `.cursor/plans/install_systemd_script_ca560f0b.plan.md` (do not edit)

## Plan mode summary

- Root **`./install`** with **`--systemd`** and optional **`--build`** (runs `./release`).
- Copies **`target/release/{tddy-daemon,tddy-coder,tddy-tools}`** to **`INSTALL_BIN_DIR`** (default `$INSTALL_PREFIX/bin`, prefix default `/usr/local`).
- Installs production config from **`daemon.yaml.production`** only if **`$INSTALL_CONFIG_DIR/daemon.yaml`** is absent (placeholder **`__INSTALL_BIN_DIR__`** substituted).
- Writes generated **`tddy-daemon.service`** to **`INSTALL_SYSTEMD_DIR`** with **`ExecStart`** using resolved paths.
- **`INSTALL_NO_SYSTEMCTL=1`**: skip root check and **`systemctl`** (tests / CI).
- Tests: **`packages/tddy-e2e`** — static contracts + functional runs in a temp tree (copy `install` + template + fake binaries).

## Affected packages / paths

- Repo root: `install`, `daemon.yaml.production`
- `packages/tddy-e2e`: `src/install_contract.rs`, `tests/install_script.rs`, `src/lib.rs`
- `docs/dev/tddy-daemon.service.example`, `AGENTS.md`, `CLAUDE.md`

## Milestones

1. Changeset + contract module + tests + script + template + doc table updates.
2. `./verify` / `./test` green.

## Acceptance

- [x] ENV overrides relocate all artifacts; `INSTALL_NO_SYSTEMCTL=1` allows non-root test harness (covered by `install_script` tests).
- [x] `cargo test -p tddy-e2e --test install_script` passes (11 tests).
- [ ] `sudo ./install --systemd` on a real host (operator smoke test before production use).

## Validation results

| Check | Result |
|--------|--------|
| `cargo fmt --all` | PASS |
| `cargo clippy -p tddy-e2e --all-targets -- -D warnings` | PASS |
| `./dev bash -c './verify'` (full workspace `cargo test`, output in `.verify-result.txt`) | PASS (2026-03-23) |
| Cursor `/validate-changes`, `/validate-tests`, `/validate-prod-ready`, `/analyze-clean-code` | Not run as slash-commands in this session — run per team workflow before merge if required |

**Status:** Implementation complete; operator smoke test of real `systemctl` path remains optional.

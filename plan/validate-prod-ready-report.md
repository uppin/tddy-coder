# Validate Prod-Ready Report — Web-Dev Daemon-Only Refactor

**Scope:** Changed/new artifacts for the web-dev daemon-only work: `web-dev`, `packages/tddy-e2e/src/web_dev_contract.rs`, `packages/tddy-e2e/tests/web_dev_script.rs`, `dev.daemon.yaml` (header/docs).  
**Tests:** This review is analysis-only; **tests were not executed** for this validation step.

---

## Executive summary

The refactor correctly centralizes on `tddy-daemon` with `DAEMON_CONFIG` / `dev.daemon.yaml`, keeps `set -euo pipefail`, temp-config handling with `EXIT` trap, and structured daemon/Vite startup. Production-adjacent risks are **behavioral and operational** rather than cryptographic: aggressive **port-based process termination** (`fuser -k -9`) can disrupt unrelated services on the same ports; **duplicate `-c` arguments** when users pass CLI flags after the script’s own `-c "$TMP_CONFIG"` may confuse the daemon’s effective config. The Rust contract module is appropriate for automated checks but mixes **assert/panic** failure modes with **`log`** calls that will often produce **no output** unless a logger is initialized in the test/binary context. **Severity overall: medium** (aligned with `plan/evaluation-report.md`), with clear mitigations (documentation, optional deduplication of `-c`, narrowing contract string matchers over time).

---

## Findings by category

### Error handling

| Finding | Severity | Notes |
|--------|----------|--------|
| `set -euo pipefail` at top of `web-dev` | **info** | Good baseline for failing fast on errors and unset variables. |
| Temp config + `trap 'rm -f "$TMP_CONFIG"' EXIT` | **low** | Registered after successful `mktemp`; cleans up on normal exit. If `sed` fails, `set -e` aborts and `EXIT` still runs—appropriate. |
| Missing config file | **low** | Clear message and `exit 1` before daemon start. |
| Daemon readiness loop (curl, 180× ~0.5s, dual 200 check) | **low** | Reduces race; failure path kills daemon PID and exits 1 with actionable stderr. |
| `wait -n` | **low** | Requires Bash with `wait -n` support; acceptable for a `#!/usr/bin/env bash` dev script targeting modern Bash. |
| `cleanup` trap + final `cleanup` call | **info** | Ensures shutdown even when one child exits; `INT`/`TERM` covered. |
| `web_dev_contract::verify_*` uses `assert!` / `panic!` | **medium** | Fits test/contract use but is not `Result`-based; any future non-test caller would get panics on contract violation. Document or gate as test-only if API surface grows. |
| Integration test `read_web_dev` | **low** | Panics on missing file—acceptable for tests pointing at fixed repo layout. |

### Logging

| Finding | Severity | Notes |
|--------|----------|--------|
| `log::info!` / `log::debug!` in `web_dev_contract.rs` | **low** | `log` is a facade; without `env_logger` (or similar) initialization in the **library** path, messages are typically discarded in normal `cargo test` runs unless the harness initializes logging. No functional breakage; debugging contract failures may lack log lines. |
| `web-dev` uses `echo` to stderr for errors | **info** | Appropriate for a standalone shell script (not TUI). |

### Config (`dev.daemon.yaml` header)

| Finding | Severity | Notes |
|--------|----------|--------|
| Header documents `DAEMON_CONFIG`, clarifies `CONFIG` from `.env` is not the daemon backend | **info** | Consistent with script behavior (`CONFIG` variable holds daemon YAML path for this script; `.env` does not override already-set vars). Review header only if operators still confuse `CONFIG` vs `DAEMON_CONFIG`—the script comment block is the primary reference. |

### Security

| Finding | Severity | Notes |
|--------|----------|--------|
| `sed "s/CURRENT_USER/${USER}/g" "$CONFIG" > "$TMP_CONFIG"` | **medium** | **`USER` is interpolated into the sed expression.** Characters special to `sed` replacement (e.g. `&`, `\`, newlines) or delimiter `/` in `USER` can mangle or break substitution. Uncommon for real usernames but is a real injection-style footgun for edge cases. Safer patterns: `sed` with alternate delimiter, or env-based substitution outside sed. |
| `./dev "$binary" "${daemon_args[@]}"` | **info** | Array expansion is properly quoted; reduces injection via word splitting. |
| `./dev bash -c "cd packages/tddy-web && DAEMON_PORT='$DAEMON_PORT' npx vite --port '$VITE_PORT' --host '$WEB_HOST'"` | **low** | `DAEMON_PORT`, `VITE_PORT`, `WEB_HOST` are single-quoted inside the double-quoted `-c` string—good for typical values. A **`'` inside `WEB_HOST`** would break the shell string (unlikely for valid hostnames). |
| `.env` loading loop | **low** | Classic limitations: lines with `=` in values, multiline values, and `export "$key=$value"` edge cases. Pre-existing pattern; not introduced solely by this refactor. |
| `fuser -k -9 ... "${DAEMON_PORT}/tcp" "${VITE_PORT}/tcp"` | **high** | **Kills whatever owns those TCP ports** with SIGKILL (`-9`). On a shared machine or if ports collide with another project, this can terminate **unrelated** processes. Dev convenience vs. safety tradeoff—call out in operator docs. |

### Performance

| Finding | Severity | Notes |
|--------|----------|--------|
| Curl polling loop (up to ~90s) | **negligible** | Expected for local dev; not a hot path for production servers. |
| Contract tests: read small text file, spawn `bash -n` | **negligible** | Suitable for CI. |

### Duplicate `-c` pass-through (from evaluation report)

| Finding | Severity | Notes |
|--------|----------|--------|
| `daemon_args+=("-c" "$TMP_CONFIG")` then `daemon_args+=("$@")` | **medium** | User can pass `-c` again (e.g. `./web-dev -c other.yaml`). Behavior depends on **`tddy-daemon`’s argument parsing** (last wins vs. error). Document expected behavior or strip/normalize duplicate `-c` in a follow-up if users report confusion. |

---

## Severity tags (summary)

- **high:** Port-wide `fuser -k -9` can kill non–tddy processes on those ports.
- **medium:** `sed` substitution with unescaped `USER`; duplicate `-c` CLI semantics; contract APIs using panic vs. `Result`.
- **low:** `log` facade without guaranteed logger init; `.env` parsing limits; `wait -n` / Bash portability; substring contract tests (legacy token in comments could false-positive—per evaluation report).
- **info:** `set -euo pipefail`, quoting of daemon args, `dev.daemon.yaml` documentation alignment.

---

## Recommendations

1. **Document** duplicate `-c` and `fuser` behavior in operator-facing docs or `web-dev` header so users know risks on shared hosts and how daemon resolves multiple `-c` flags.
2. **Harden `CURRENT_USER` replacement** (escape delimiter or use a non-sed path) if usernames with special characters must be supported.
3. **Optional:** Initialize `env_logger` in `tddy-e2e` test main or use `tracing` + test subscriber only in test binaries—only if richer contract failure diagnostics are needed.
4. **Optional:** Expose `verify_*` as `-> Result<(), E>` for non-test callers, keeping thin `assert`-based wrappers for tests—if the module’s public API must stay panic-free.
5. **Hygiene (from evaluation report):** Do not ship `.green-web-dev-test-output.txt` / `.red-web-dev-test-output.txt`; trim unrelated rustfmt-only diffs where possible.

---

## References

- `plan/evaluation-report.md` — baseline risk, duplicate `-c`, contract substring caveat.
- Artifacts reviewed: `web-dev`, `packages/tddy-e2e/src/web_dev_contract.rs`, `packages/tddy-e2e/tests/web_dev_script.rs`, `dev.daemon.yaml`.

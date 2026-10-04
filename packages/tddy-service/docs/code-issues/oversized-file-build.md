# oversized-file: build.rs — the crate's proto code generation script

**Location:** `packages/tddy-service/build.rs`
**Category:** oversized-file
**Detected:** 2026-09-24 by the `/pr-wrap` file-length gate on #510 (`#keyring` 3/9)
**Metrics:** **723 production lines** (2026-10-04, HEAD of #511; 709 on `origin/master` `b42eb558`; 695 at #510, 692 before it) — the file has no `#[cfg(test)]`, so every line counts · budget 500 · ~1.39× over · `main` alone is **662 lines** (2026-10-04, from `:4`; 648 before #511, 634 at #510), one `prost_build::Config` / `tonic_build` chain per proto family
**Thresholds breached:** length 723 > 500; `main` 662 > 60
**Restructure:** required — function splitting inside a build script (see below)
**Status:** Open — regressed 2026-10-04 (+14 from #511, `#keyring` 4/9; +14 from #574). Pre-existing over budget; #510 grew it by three lines and deferred the split with the developer's consent, to a follow-up after the `#keyring` stack lands — `docs/dev/todo/2026-09-24-keyring-store-deferred-oversized-file-splits.md`

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-09-24 | 692 → 695 | `origin/master` `35cf2913` → #510 HEAD: `+3`, the passphrase `Debug` redaction (N1). The auth block gains `.skip_debug([".auth.UnlockVaultRequest", ".auth.ResetVaultRequest"])` and a two-line comment pointing at `src/auth_redacted_debug.rs`, so prost derives no `Debug` that would print the vault passphrase. Grown; deferred with consent |
| 2026-10-03 | 709 | 695 → 709 after #574 (`#live-plan` 8/15): one more `prost_build` / RPC-server pass for `code_navigation.proto`, in `main`. Grown; the split is deferred with the developer's consent (`docs/dev/todo/2026-10-03-code-navigation-grew-four-oversized-files.md`) |
| 2026-10-04 | 723 | 709 on `origin/master` (merge-base `b42eb558`) → 723 after #511 (`#keyring` 4/9): +14, one more single-pass `prost_build` / RPC-server block for `accounts.proto` (no tonic adapter) and its four-line comment, in `main` (648 → 662). Grown; the split is deferred with the developer's consent (2026-10-04) to `docs/dev/todo/2026-09-24-keyring-store-deferred-oversized-file-splits.md`, because open PRs #63, #68, #118 and #178 also edit this file |

## What the gate found

Already 692 lines on `origin/master` before #510 touched it. The whole file is one `main` plus three
small helpers (`ensure_exec_tools_tonic_adapter`, `tonic_service_module`, `rust_module`). `main` is
a sequence of about thirty independent code-generation blocks, one per `.proto` (or per family of
them, looped), each building its own `prost_build::Config` with its own attributes and service
generator.

**#510's contribution is three lines** — see the history row. It is the smallest change that keeps
the passphrase out of a derived `Debug`: the redacting `impl Debug` itself lives in
`src/auth_redacted_debug.rs`, not here.

## What would close it

The blocks do not share state beyond `OUT_DIR`, so `main` splits along them without a design
question: one `fn compile_<family>() -> Result<(), Box<dyn Error>>` per group (the `connection`
family, the terminal pair, the auth/token pair, the sandbox pair, the reflection block, the
single-proto services), with `main` calling them in today's order. Moving the groups into a
`build/` module directory (`#[path]`-included from `build.rs`) takes the file under 500 and every
function under 60.

⚠ Two protos are compiled twice — `terminal.proto` (`:88`, `:105`) and `sandbox.proto` (`:482`,
`:550`), once by prost and once by `tonic_build` into its own directory — so keep each pair in one
group. Prove the split with
`./dev scripts/generated-code.sh check` and a clean `cargo build -p tddy-service` before and after:
a build script has no tests of its own, and the generated code is the behaviour.

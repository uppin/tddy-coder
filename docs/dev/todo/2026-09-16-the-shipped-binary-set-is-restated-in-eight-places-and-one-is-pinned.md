# 2026-09-16 — The shipped binary set is restated in eight places and only one is pinned

**Category:** Test infrastructure
**Source:** `2026-09-15-warm-code-intelligence-daemon` changeset — adding `tddy-index-daemon` broke
29 tests in four suites, none of them about the index daemon

Adding **one** binary to the product broke 29 tests across `tddy-e2e`, `tddy-vm-testkit` and the
cloudinit VM leg. Not one of those failures was a defect in the new binary, in `./install`, or in the
change that shipped it: every one was a *different copy of the same list* that nobody updated.

Where the shipped set is stated:

| Site | Kind | Pinned to `install`? |
|---|---|---|
| `install:561` `INSTALLED_BINARIES` | **source of truth** | — |
| `release:15` `cargo build --release -p …` | production | no |
| `publish.sh:64`, `:120`, plus prose at `:24` and `:169` | production | no |
| `packages/tddy-vm-testkit/src/recipes.rs` `deployed_binaries()` | production | **yes** |
| `packages/tddy-e2e/tests/install_script.rs:140` `write_fake_release_binaries` | fixture | no |
| `packages/tddy-e2e/tests/install_supervisor.rs:51` `an_install_tree` | fixture | no |
| `packages/tddy-e2e/tests/sandbox_runner_shipping_acceptance.rs:71` `RELEASE_BINARIES` | fixture | no |
| `.github/workflows/ci.yml` "Stage the deployment set" | CI staging | no |

The three production scripts were all updated by the change that shipped the binary. **Every site
that was missed was a fixture or CI staging** — and the one list that caught itself is the one with a
test behind it: `recipes_unit::deploys_every_binary_the_install_script_requires` parses
`INSTALLED_BINARIES` out of `install` and diffs it, so it failed with the actual answer:

```
./install requires tddy-index-daemon, which the deployed set does not carry: [...]
```

That is the difference worth generalising. The other seven sites fail by *symptom* instead:

- the e2e fixtures fail as `install should succeed with test env; got ExitStatus(256)` — 26 tests,
  none of which reaches its own assertion, all of them saying nothing about the cause;
- the cloudinit leg fails only after a bake, as `dist is not a complete dist directory`.

## What closing it looks like

Make the fixtures read the source of truth rather than restate it. `recipes_unit` already shows the
technique and it is a few lines: parse `INSTALLED_BINARIES` from `install` and stage exactly those
names. Then a new binary needs one edit to `install`, and no fixture can drift from it.

Two sites need care rather than a mechanical change:

- `sandbox_runner_shipping_acceptance`'s `RELEASE_BINARIES` is documented as "deliberately the full
  set plus the runner: an install that silently skipped an unexpected file would pass a narrower
  fixture". Deriving it must keep that property — it wants `INSTALLED_BINARIES` **plus** extras, not
  whatever the script happens to name.
- `install_fails_without_binaries` asserts the preflight abort on an empty `target/release`. It must
  keep passing, so the derivation cannot be applied to that case.

`ci.yml` cannot import Rust, so it stays a restatement — but it is the one site a test could cover
cheaply, by asserting the workflow's list against `install`'s the way `recipes_unit` does. Its
comment already claims it mirrors `recipes::deployed_binaries`, "which is itself pinned against
./install's INSTALLED_BINARIES by a unit test" — the mirror is what is unpinned.

## Also worth knowing

`fixture-bins` in the same workflow is a **different** set with a different rule — the binaries
integration tests shell out to, "everything matched by `grep -r 'target/debug/' packages/*/tests`".
`tddy-index-daemon` correctly does **not** belong in it: `tddy-daemon` resolves the program through
`CARGO_BIN_EXE_tddy-index-daemon` or as a sibling of its own executable
(`packages/tddy-daemon/src/index_daemon/spawn.rs:45`), never by path into `target/debug`. Do not
collapse the two lists.

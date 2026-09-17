# 2026-09-16 — Three comments claim `./release` does not build `tddy-sandbox-runner`, and one build step rests on that

**Category:** Stale documentation / redundant work
**Source:** `2026-09-15-warm-code-intelligence-daemon` changeset — found while adding
`tddy-index-daemon` to the four lists that pin the shipped binary set

`release:15` builds ten packages, `-p tddy-sandbox-runner` among them:

```
cargo build --release -p tddy-coder -p tddy-tools -p tddy-sandbox-darwin -p tddy-sandbox-app \
  -p tddy-sandbox-runner -p tddy-daemon -p tddy-supervisor -p tddy-remote-git-repo \
  -p tddy-session-sync -p tddy-index-daemon
```

`sandbox_runner_shipping_acceptance::the_release_script_builds_the_sandbox_runner_every_jail_spawns`
passes precisely because it does. Three comments say otherwise:

| Site | What it says |
|---|---|
| `packages/tddy-vm-testkit/tests/recipes_unit.rs:236` | "even though `./release` does not build it" |
| `packages/tddy-vm-testkit/src/builder_vm.rs:258–259` | "`./release` does not build the jailed payload" |
| `packages/tddy-e2e/tests/sandbox_runner_shipping_acceptance.rs:7–8` | "`./release` builds six binaries and this is not one of them; `./install` ships five and `publish.sh` packages five" |

The third describes the state its **own** 2026-08-30 changeset then changed, so it was stale on
arrival. The counts in it are wrong twice over now — `./release` builds ten and `./install` ships
seven.

## The part that is more than prose

`builder_vm.rs:260–269` builds `tddy-sandbox-runner` in the guest as an explicit extra step,
justified by the false comment beside it. Since `./release` already builds it, that step is
redundant — an extra incremental `cargo build --release` inside a VM bake.

It was left in place deliberately rather than removed: no test demands its removal, and the code
runs only inside `#[ignore]`d VM tests that need `TDDY_CLOUDINIT_BASE_IMAGE` and a multi-hour
image-chain bake, so the change could not be executed locally to prove it safe. Removing a build
step on a hunch, where the feedback loop is hours long and the failure mode is a missing binary
inside a guest, is worse than the redundancy.

## What closing it looks like

Correct the three comments — they are the actual harm, since each one tells the next reader to add
a build step that already exists. Then, in a change that is **already** running the VM suites for
another reason, drop the `builder_vm.rs` step and confirm the bake still produces a guest carrying
every binary `deployed_binaries()` names. Doing it as its own change means paying the bake purely
to remove a duplicate compile.

`deployed_binaries()` (`packages/tddy-vm-testkit/src/recipes.rs`) is the list to check it against,
and `recipes_unit::deploys_every_binary_the_install_script_requires` reads `INSTALLED_BINARIES`
straight out of `install`, so that assertion stays honest without being restated here.

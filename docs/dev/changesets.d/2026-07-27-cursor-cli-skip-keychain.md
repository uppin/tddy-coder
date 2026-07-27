# 2026-07-27 — cursor-cli-skip-keychain

**Type:** Bug Fix · **Branch:** `feat-keychain-access`
**Packages:** `tddy-sandbox-recipes`, `tddy-daemon`
**Index line:** [docs/dev/changesets.md](../changesets.md)
**Features:** [cursor-cli-session.md § Known follow-ups](../../ft/daemon/cursor-cli-session.md#known-follow-ups)
**Per-package:** [tddy-sandbox-recipes](../../../packages/tddy-sandbox-recipes/docs/changesets.md) ·
[tddy-daemon](../../../packages/tddy-daemon/docs/changesets.md)

## What was broken

A sandboxed Cursor Agent CLI session on macOS hung for ~30s on `securityd` and failed with
`Failed to store authentication tokens: Security command failed: Security process exited with code: 36`
(or `errSecMissingEntitlement` / -34018). The session never reached the ready marker, so
`start_sandboxed_cursor_cli_session` timed out with `deadline_exceeded`.

Two independent causes:

1. **No Keychain opt-out in the cursor sandbox env.** Cursor's official [Authentication
   docs](https://cursor.com/docs/cli/reference/authentication) list only browser login
   (stores tokens in macOS Keychain as the `cursor-user` item) and `CURSOR_API_KEY`. Even with
   `CURSOR_API_KEY` set, the Cursor CLI still probes Keychain for `cursor-user` on macOS in
   certain code paths (confirmed by forum reports and the community
   [claude-overnight `CURSOR_PROXY_MACOS_DISCOVERY.md`](https://github.com/Fornace/claude-overnight/blob/master/docs/CURSOR_PROXY_MACOS_DISCOVERY.md)).
   The tddy sandbox host is an unsigned Rust binary with no `keychain-access-group`
   entitlement and no stable signing identity for TCC attribution, so the `securityd` Mach
   round-trip returns `errSecMissingEntitlement` without a prompt. The SBPL profile already
   permits `mach-lookup` (`MachPolicy::All` in `claude_interactive_policy`, reused by
   `cursor_interactive_policy`), so the Mach channel was never the blocker — entitlements were.
2. **`cursor_runner_env_overlay` was defined but never called from the cursor sandbox spawn
   path.** `build_sandbox_runner_env` (in `sandbox_session.rs`) unconditionally extends with
   `claude_runner_env_overlay` — so `CLAUDE_CODE_TMPDIR`/`CLAUDE_TMPDIR` were set even in
   cursor sessions — but never with `cursor_runner_env_overlay`. So even the existing
   `CURSOR_TMPDIR` was never set in cursor sandbox sessions before this fix.

The repo's own follow-up note in `docs/ft/daemon/cursor-cli-session.md` "Known follow-ups"
referenced `AGENT_CLI_CREDENTIAL_STORE=file` as a workaround. That environment variable does
not exist for Cursor CLI — it appears to be a confusion with Claude Code's credential-store
knob. The actual Cursor-supplied opt-out is `CURSOR_SKIP_KEYCHAIN=1` (used by Cursor's own
CI), paired with `CI=true` to prevent a parent shell from re-enabling interactive probes.

## What was decided

- **Bake the Keychain skip into `cursor_runner_env_overlay` unconditionally, not as a
  per-call knob.** The overlay's purpose is "env for a sandboxed cursor runner", and a
  sandboxed cursor runner *cannot* reach Keychain (unsigned host, no entitlements). Making it
  opt-in would imply there is a sandboxed cursor scenario where Keychain access works, which
  there is not. Baking it in matches the repo's "no fallbacks" rule — the caller still has to
  supply `CURSOR_API_KEY` or seeded `auth.json`, and a missing credential fails fast with
  "not authenticated" instead of hanging on `securityd`.
- **Set `CI=true` alongside `CURSOR_SKIP_KEYCHAIN=1`.** `CI=true` is Cursor's own convention
  to prevent a parent shell from re-enabling interactive probes. Setting it in a production
  sandbox session is not a "fallback" — it is the correct environment for a headless sandbox,
  identical to Cursor's CI. The alternative (only `CURSOR_SKIP_KEYCHAIN=1`) leaves a
  re-enablement vector open.
- **Extract `build_sandboxed_cursor_runner_env` as a pure helper in `sandbox_session.rs`.**
  The env-assembly step of `start_sandboxed_cursor_cli_session` was inline and untestable
  without a real Seatbelt/cgroups harness. Extracting it into a pure function (no `&self`
  dependencies) makes the Keychain-skip behavior unit-testable directly. Specialized-subagent,
  LSP, and semantic-index overlays remain inline in the spawn path (they depend on `&self` /
  runtime state).
- **Do not touch `build_sandbox_runner_env`'s unconditional `claude_runner_env_overlay`.**
  Pre-existing latent bug (Claude env vars leak into cursor sessions), but fixing it requires
  restructuring the helper and is orthogonal to the Keychain fix. Recorded as a follow-up,
  not silently fixed in this changeset.
- **Do not enable Keychain *access* from the jail.** That would require codesigning
  `tddy-sandbox-app` with `keychain-access-group` and a TCC grant — a separate, larger
  effort. This changeset removes a path that cannot work; the caller still has to supply
  `CURSOR_API_KEY` or pre-seeded `~/.cursor/auth.json`.

## What changed

### tddy-sandbox-recipes

`cursor_runner_env_overlay` (`packages/tddy-sandbox-recipes/src/cursor_cli.rs`) now emits
`CURSOR_SKIP_KEYCHAIN=1` and `CI=true` alongside `CURSOR_TMPDIR`, with a doc comment
explaining the entitlement blocker and the caller's remaining `CURSOR_API_KEY`/seeded-
`auth.json` responsibility.

Two fluent-style unit tests in `cursor_cli::tests`:
- `cursor_runner_env_overlay_redirects_cursor_tmpdir_into_the_scratch_tree`
- `cursor_runner_env_overlay_skips_macos_keychain_so_the_agent_never_probes_securityd`

### tddy-daemon

New pure helper `build_sandboxed_cursor_runner_env` in `sandbox_session.rs` —
`build_sandbox_runner_env` + `cursor_runner_env_overlay`. No `&self` dependencies, so the
Keychain-skip behavior is unit-testable without a Seatbelt/cgroups harness.

`start_sandboxed_cursor_cli_session` (`connection_service.rs`) calls the helper instead of
inline env assembly (`build_sandbox_runner_env` + manual `env.extend(cursor_runner_env_overlay(...))`).
Specialized-subagent / LSP / semantic-index extensions remain inline.

One fluent-style unit test in `sandbox_session::tests`:
- `cursor_runner_env_skips_keychain_so_the_jailed_agent_never_probes_securityd` — asserts
  `CURSOR_SKIP_KEYCHAIN=1`, `CI=true`, and `CURSOR_TMPDIR` are present in the cursor sandbox
  env, with no jail spawn required.

### docs/ft/daemon/cursor-cli-session.md

The "Known follow-ups" bullet on jail authentication is rewritten: the bogus
`AGENT_CLI_CREDENTIAL_STORE=file` reference is replaced with the real recipe
(`CURSOR_SKIP_KEYCHAIN=1` + `CI=true` + `CURSOR_API_KEY` or pre-seeded `~/.cursor/auth.json`),
the entitlement blocker is named (`errSecMissingEntitlement` / -34018), and the codesigning
follow-up for true Keychain *access* is recorded.

## Verification

- `cargo test -p tddy-sandbox-recipes --lib cursor_runner_env_overlay` — 2/2 pass.
- `cargo test -p tddy-daemon --lib sandbox_session::tests::cursor_runner_env_skips_keychain` — 1/1 pass.
- `cargo build -p tddy-daemon` — clean.
- `cargo clippy -p tddy-sandbox-recipes -p tddy-daemon -- -D warnings` — clean.
- `rustfmt --check` on changed files — clean.
- One pre-existing unrelated failure remains: `tddy-sandbox-recipes`
  `cursor_cli::tests::cursor_agent_prerequisite_reads_include_install_dir_and_share_root`
  asserts a macOS-only `/Users` traversal root and cannot pass on Linux (this host's `HOME`
  is `/var/tddy`). Documented in the `2026-07-26 pr-stack-branch-gated-spawn` changeset
  entry; not introduced or affected by this changeset.

## Follow-ups carried forward

- `build_sandbox_runner_env` unconditionally applies `claude_runner_env_overlay` even for
  cursor sessions (Claude env vars leak into cursor sessions). Harmless but incorrect; fixing
  it requires parameterizing the helper or moving the Claude overlay into the claude spawn
  paths.
- True Keychain *access* from the jail (not just suppressing the probe) requires codesigning
  `tddy-sandbox-app` with `keychain-access-group` and a TCC grant. Current stance: fail
  closed with `CURSOR_API_KEY`/seeded `auth.json`.
- `CURSOR_SKIP_KEYCHAIN` is undocumented in Cursor's public auth docs (it appears in
  Cursor's own CI scripts and in the claude-overnight discovery doc). If Cursor removes it,
  sandboxed cursor sessions will resume hanging on Keychain. Mitigated by the doc comment
  naming the source and by `CI=true` (which Cursor documents for headless).
- End-to-end daemon integration test for `start_sandboxed_cursor_cli_session` (full
  Seatbelt/cgroups spawn, real git repo, real cursor binary) is not added in this changeset.
  The Keychain-skip env behavior is pinned by the pure-helper unit test at the daemon layer
  plus the two recipe-layer unit tests; the full spawn path remains covered by the existing
  `sandboxed_claude_cli_acceptance.rs`-style harness pattern (not extended to cursor here).

## References

- [Cursor Authentication docs](https://cursor.com/docs/cli/reference/authentication)
- [Cursor CLI headless docs](https://cursor.com/docs/cli/headless)
- [Fornace/claude-overnight `CURSOR_PROXY_MACOS_DISCOVERY.md`](https://github.com/Fornace/claude-overnight/blob/master/docs/CURSOR_PROXY_MACOS_DISCOVERY.md)
- [Cursor forum: "Cursor CLI login via SSH/Mosh"](https://forum.cursor.com/t/cursor-cli-login-via-ssh-mosh/149045)
- [Cursor forum: "Cursor CLI is entirely broken via SSH"](https://forum.cursor.com/t/cursor-cli-is-entirely-broken-via-ssh/141835)
- [zed-industries/zed#55286](https://github.com/zed-industries/zed/issues/55286)
- Repo: [darwin-sandbox skill](../../../.agents/skills/darwin-sandbox/SKILL.md)
- Repo: [packages/tddy-sandbox-darwin/src/profile.rs](../../../packages/tddy-sandbox-darwin/src/profile.rs) — SBPL profile rendering, `MachPolicy::All` for agent recipes.
- Repo: [packages/tddy-sandbox-recipes/src/claude_cli.rs](../../../packages/tddy-sandbox-recipes/src/claude_cli.rs) — `claude_interactive_policy` (reused by `cursor_interactive_policy`).

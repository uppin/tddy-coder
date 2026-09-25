# 2026-09-25 — a cross-crate move does not carry an external crate the moved file names only in a body path

**Category:** Future enhancement (engine defect; the tree does not compile after the move)
**Source:** `#carve` 15/15, [#526](https://github.com/uppin/tddy-coder/pull/526), plan
`docs/dev/1-WIP/2026-09-23-carve-lifecycle-wiring-plans/02a-pty-runtime-to-terminal-rpc.jsonl`,
op 0 (`move_cluster_to_crate`: `pty_runtime` + `tddy_user_config` → `tddy-terminal-rpc`)

## What happened

`tddy_user_config.rs`'s inline test module never imports `tempfile`. It names it as a qualified
path in a function body:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_returns_none() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(spawn_path_extra_for_home(tmp.path()), None);
    }
}
```

The origin, `tddy-session-lifecycle`, has `tempfile = "3"` in `[dependencies]`. The destination
has no `tempfile` at all. The manifest pass carried `tddy-daemon-kernel` (named in a `pub use`
line), but not `tempfile`, so the destination's test build failed:

```text
packages/tddy-terminal-rpc/src/tddy_user_config.rs:18:19: error[E0433]: failed to resolve: use of unresolved module or unlinked crate `tempfile`
packages/tddy-terminal-rpc/src/tddy_user_config.rs:34:19: error[E0433]: failed to resolve: use of unresolved module or unlinked crate `tempfile`
error: could not compile `tddy-terminal-rpc` (lib test) due to 2 previous errors
```

`apply` reported `1 of 1 operation(s) were applied, and the tree no longer compiles`.

## Why

The manifest pass seems to read the crates a moved file names from its `use` declarations. The
[known limitations](../../ft/coder/rust-code-restructuring.md#known-limitations) already say the header
pass is mechanical and reads only `use` heads. A crate reached only as `krate::item(…)` inside a body
is invisible to it, and so is a crate used only under `#[cfg(test)]`.

## What was fixed by hand

```toml
# packages/tddy-terminal-rpc/Cargo.toml
[dev-dependencies]
# `tddy_user_config`'s tests write a `.tddy/config.yaml` under a temporary home.
tempfile = "3"
```

It went into `[dev-dependencies]`, not `[dependencies]` where the origin had it, because only the
test module uses it.

## What would fix it

The manifest pass could collect the first segment of every path in the moved file, bodies included,
and look each up against the origin's manifest. A path first seen inside a `#[cfg(test)]` item would
go to `[dev-dependencies]`. The same collection would catch a crate named in a non-test body too,
which fails the lib build rather than only the test build.

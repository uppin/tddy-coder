# `tddy-coder:bin`'s `srcs` do not cover `tddy-coder-cli`

**Recorded:** 2026-10-21, by `/pr-wrap` on #690 (`#satellite` 15/15)
**Area:** `packages/tddy-coder/BUILD.yaml`, BSP target `tddy-coder:bin`

## What is wrong

Since #630 the `tddy-coder` binary is built by `tddy-coder-cli`. #690 corrected the target's
`package:` to `tddy-coder-cli`, but its `srcs` still list only the library:

```yaml
      package: tddy-coder-cli
      bin_name: tddy-coder
      srcs:
        - "packages/tddy-coder/src/**/*.rs"
        - "packages/tddy-coder/Cargo.toml"
```

A change to `packages/tddy-coder-cli/src/main.rs` or its manifest therefore does not invalidate the
target, and a BSP build can report the old binary as current.

## Why it was not fixed in #690

The developer approved the `package:` correction only. `srcs` is the same defect, but it was not
part of that approval or of #690's changeset.

## What would close it

Add the wiring crate to `srcs`, as #690 did for `packages/tddy-tools/BUILD.yaml`:

```yaml
        - "packages/tddy-coder-cli/src/**/*.rs"
        - "packages/tddy-coder-cli/Cargo.toml"
```

It is a two-line change. Any PR that touches `tddy-coder`'s build can take it, or the `#satellite`
sweep node can.

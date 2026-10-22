# 2026-10-20 — `tddy-zed-extensions`: install, list and remove Zed extensions

**Type:** Feature

`#reshape` 19/24 ([#678](https://github.com/uppin/tddy-coder/pull/678)). New crate
`packages/tddy-zed-extensions`; `tddy-tools` gains `Subcommand::Extensions`, whose args and `run` live
in the crate (`cli.rs`, the `restructure_args.rs` pattern). Feature doc:
[Zed extensions](../../ft/coder/zed-extensions.md).

## Layout

| Module | Owns |
|---|---|
| `dir.rs` | `ExtensionsDir` — `<data dir>/extensions`; `resolve()` (env, CLI only), `resolve_from(tddy_data_dir, home)` |
| `manifest.rs` | `ExtensionManifest` and its entries (serde over `extension.toml`), `check_schema_version` |
| `registry.rs` | `Registry` — `at(api)`, `from_env()` (`TDDY_ZED_EXTENSIONS_API`, CLI only) |
| `index.rs` | `Index` / `IndexEntry` / `ExtensionSource`; `save` is temp-file + fsync + rename |
| `archive.rs` | tar unpack with per-entry containment checks, then a real-resolution pass over every symlink |
| `install.rs` | `install_from_registry`, `install_from_path`, `list`, `remove`, validation (R3), the R5 decision |
| `staging.rs` | `Staging` (`installed/.<id>.partial`, removed on drop), `swap_in` (previous tree moved to `.<id>.previous`, restored on a failed promote or index save), `copy_tree`, `remove_tree` |
| `cli.rs` | clap args; `run` resolves env and prints what `execute(dir, registry, command)` returns |

Library functions take the directory and registry explicitly; only `cli::run` reads the environment, so
tests never race on process env.

## Decisions made at wrap (`/validate-changes`)

- Registry downloads carry a 120 s whole-request timeout (`REGISTRY_TIMEOUT`).
- Language `config.toml` files are parsed at install, so `list` cannot meet one it cannot read.
- Symlink safety is lexical per entry **and** checked by `canonicalize` after unpack: a chain of
  in-tree links resolving outside (or a dangling link) is refused.
- The swap is move-aside-then-rename with rollback, so R2's "previous install untouched on any refusal"
  holds through a failed rename or index save.
- `cli::execute` split out of `run` so the CLI arms are tested (`tests/cli_acceptance.rs`); `run` CRAP
  156 (complexity 12, unexecuted) → the remaining `run` is a three-line printer.
- `install.rs` stayed under 500 production lines by moving the staging/swap/copy helpers to `staging.rs`
  (`install.rs` was not modified by K20–K24).

## Verification

`cargo test -p tddy-zed-extensions --no-fail-fast` — 21 passed (3 unit, 15 install acceptance, 3 CLI
acceptance); `cargo clippy -p tddy-zed-extensions -p tddy-tools --all-targets -- -D warnings` — clean.

No backlog entries or code-issue records were resolved.

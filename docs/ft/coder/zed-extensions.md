# Zed extensions

**Status:** 🚧 Install, list and remove delivered (`#reshape` 19/24, [#678](https://github.com/uppin/tddy-coder/pull/678)); nothing loads an installed extension yet.

tddy reuses Zed editor extensions — their languages, tree-sitter grammars (already compiled to WASM)
and language-server declarations — to understand languages it has no built-in support for. This
document covers getting an extension onto the machine.

## Where extensions live

The **extensions directory** is `<tddy data dir>/extensions`, the data dir resolved as the daemon
resolves it: `TDDY_DATA_DIR` when set and non-blank, else `~/.tddy` (a debug build defaults to the
checkout's `tmp/.tddy`). It is per user.

```
<data dir>/extensions/
  index.json                       one entry per installed extension
  installed/<id>/                  the extension, in Zed's own installed layout
    extension.toml
    languages/<language>/config.toml
    grammars/<grammar>.wasm
    extension.wasm                 optional; never run by this feature
```

The layout is Zed's, so a directory Zed installed can be copied in unchanged. `index.json` records
`id`, `version`, `source` (`registry` or `path`) and `installed_at` (Unix seconds) per extension.

## The `extensions` CLI

| Command | Does |
|---|---|
| `tddy-tools extensions install <id>[@<version>]` | Downloads the archive from Zed's extension registry (latest compatible when no version) and installs it |
| `tddy-tools extensions install --path <dir>` | Copies a local extension directory (a dev extension) in, under its manifest's id; symlinks in the source are followed |
| `tddy-tools extensions list` | One line per extension: `<id> <version> <source> — languages: …; grammars: …; language servers: …; wasm: yes\|no` (`none` for an empty list) |
| `tddy-tools extensions remove <id>` | Deletes the extension and its index entry; an unknown id is an error naming it |

Re-installing the version already installed changes nothing and says so; a different version
replaces it and names the version it replaced.

### Registry

`GET <api>/extensions/<id>[/<version>]/download` with `min_schema_version=0`, `max_schema_version=1`,
`min_wasm_api_version=0.6.0`, `max_wasm_api_version=0.7.0` — Zed's own `since_v0_6_0` range, so a
downloaded code extension is one tddy can later run. `<api>` is `https://api.zed.dev` (Zed's client
API, not documented as public), overridable with `TDDY_ZED_EXTENSIONS_API`. A non-success status is
refused with the status and the URL; a download that does not finish within 120 seconds is refused.

## Install is atomic

An install stages into `installed/.<id>.partial`, validates the staged tree, and only then swaps it in.
The previous version is moved aside, not deleted, until the new one is in place and the index is
written; any failure on the way puts it back. A refusal at any step leaves the previous install and
`index.json` as they were.

### What is refused

Each refusal names the extension and the file, relative to the extension root:

- a missing or unparsable `extension.toml`, or one whose `id` differs from the id asked for;
- a `schema_version` outside 0..=1;
- an archive entry that is absolute or climbs out with `..`, a link whose target does, an entry written
  through a link, and a symlink — alone or through a chain of links — that resolves outside the
  extension (or does not resolve at all);
- a declared grammar without `grammars/<grammar>.wasm` (tddy builds nothing);
- a `languages/<x>/` without `config.toml`, or with one that does not parse.

## Not yet

- Nothing registers an installed extension's languages, loads its grammars or starts its language
  servers.
- No `extension.wasm` is run; no grammar or extension is built.

# Zed extensions

**Status:** 🚧 Install, list and remove delivered (`#reshape` 19/24, [#678](https://github.com/uppin/tddy-coder/pull/678)); an installed extension's languages get anchors and outlines from their grammar (`#reshape` 20/24, [#679](https://github.com/uppin/tddy-coder/pull/679)); an extension language's server starts from tddy settings, with rename and references (`#reshape` 22/24, [#681](https://github.com/uppin/tddy-coder/pull/681)).

tddy reuses Zed editor extensions — their languages, tree-sitter grammars (already compiled to WASM)
and language-server declarations — to understand languages it has no built-in support for. This
document covers getting an extension onto the machine, and what `tddy-tools restructure` does with
the languages it declares.

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

## Languages from an extension

Every language an installed extension declares is a **language support** of
[`restructure`](rust-code-restructuring.md#languages), registered after the built-ins by both the
cold CLI (`tddy-tools restructure`) and the index daemon, so the two answer the same anchors.

- **What is loaded.** Each `installed/<id>/extension.toml` (the installed layout, not `index.json`;
  a failed install's `.<id>.partial` is skipped). Its languages are the manifest's `languages`, else
  every directory under `languages/`, sorted by name; each is read from its `config.toml` (`name`,
  `grammar`, `path_suffixes`) and the `outline.scm` beside it — the only query read. No `installed/`
  directory is no extensions, and then no WASM engine is started.
- **Id.** The language `name` lower-cased, each space a `-`: `"Elixir"` → `elixir`,
  `"Demo TOML"` → `demo-toml`. Errors name the language by it.
- **Claims.** The language claims the files its `path_suffixes` name — a file extension (`toml`) or a
  whole file name. A built-in keeps a suffix an extension also claims: an extension that lists `rs`
  never takes a Rust file. A whole-file-name suffix (`Cargo.lock`) is claimed but not yet routed to
  the language's anchors (known gap).
- **Grammar.** `grammars/<grammar>.wasm`, loaded through tree-sitter's WASM loader into one wasmtime
  engine per process, each grammar once per run. tddy builds no grammar.
- **Outline.** The matches of `outline.scm` over the file's parse: each `@item` is an entry named by
  its `@name`; an entry's context is the entries whose `@item` encloses it, outermost first. Its span
  is the `@item` node, extended back over leading `@annotation` nodes and ending at the node's last
  non-whitespace byte (a TOML table's node runs on over the blank lines to the next table).
- **Item paths.** `<file relative to the workspace root>::<context…>::<name>` — no module system is
  known, so the file is the prefix: `config/app.toml::server::port`.
- **Anchors.** `restructure anchors --items` / `--at` on such a file, an `item` / `items` anchor in a
  plan, a static `check`'s fingerprint test and a `snapshot`'s re-resolution work as they do for Rust, with Rust's refusals
  (non-adjacent items, a stale fingerprint, an absent or ambiguous name). None of them starts
  rust-analyzer for a file only an extension language claims.
- **No server.** A language no `[language_servers.<id>]` serves refuses every operation on its files —
  `rename_symbol`, code actions, references — with `UnsupportedOp { backend: "<language id>", op }`. A
  language with a server is described under [Language servers](#language-servers).
- **A grammar that does not load** is reported once per run, however many languages share it —
  `extension <id>: grammar <g> did not load: <reason>` — and its languages claim nothing; every other
  extension still loads. An `outline.scm` that does not compile fails its own language with the same
  report and spares a sibling on the same grammar. An unreadable `extension.toml` or language
  `config.toml` fails the run, naming the extension and the file.

**Cost.** tree-sitter's `wasm` feature brings wasmtime (46 crates). The debug `tddy-index-daemon` is
108 MB (60 MB without it); a standalone release binary that loads a grammar, parses and runs a query
is 11 MB. The release sizes of `tddy-tools` and `tddy-index-daemon` are not measured.

## Language servers

An extension's `[language_servers.<id>]` entries say **which** server serves which of its languages. Zed takes
the server's **command** from the extension's WASM; tddy runs no WASM, so it takes the command from its own
settings, in the shape of Zed's `lsp` setting — a block copied from Zed's `settings.json` works unchanged.

**Settings file.** `<extensions dir>/settings.json`. Unknown keys are ignored; a file that is not there is no
settings; one that does not parse fails the load, naming the file.

```json
{
  "lsp": {
    "elixir-ls": {
      "binary": { "path": "/opt/elixir-ls/language_server.sh", "arguments": [], "env": {} },
      "initialization_options": {},
      "settings": {}
    }
  },
  "languages": { "Elixir": { "language_servers": ["elixir-ls"] } }
}
```

- **Which server.** The manifest entry whose `language` or `languages` names the language. An entry that also
  lists the language in `opt_in_languages` serves it only when `languages.<Name>.language_servers` names it as a
  bare id — `!<id>` disables it, `...` does not name an opt-in server, and a language with no entry names nothing,
  as in Zed. When several entries qualify, the first by id serves the language (the tie-break is an open
  decision: [backlog](../../dev/todo/2026-10-22-extension-language-several-qualifying-servers-tie-break.md)).
- **Command.** Only `lsp.<id>.binary.path`, with its `arguments` and `env`. A server id is never looked up on
  `PATH`.
- **No command.** Every operation that needs the server is refused before any process starts:
  `language server <id> (extension <ext>) has no command: set lsp.<id>.binary.path in <file>`. Item anchors
  still answer from the outline.
- **Configuration.** `initialization_options` is sent at `initialize`. A `workspace/configuration` request is
  answered item by item from `settings`, by each item's `section` path, with `null` where nothing is set.
- **Language id.** Documents are opened with the entry's `language_ids` value for the language, else the
  language's id.
- **Root.** The workspace root of the run or worktree; no manifest rule is looked for.
- **Positions.** tddy asks for byte (`utf-8`) positions; a server that settles on anything else is refused
  before a rename is sent, since its columns would be mis-addressed.
- **Readiness.** A server not asked for rust-analyzer's `experimental/serverStatus` reads `READY` once
  `initialize` has completed.

**What a server gives the language:**

- **`rename_symbol`** on an `item` / `items` / `range` anchor: the item's name position, as the outline locates
  it, is sent as `textDocument/rename`, and the returned `WorkspaceEdit` is applied as Rust's edits are — by the
  cold CLI and through the index daemon. A run starts one server per installed language a file of its plan
  belongs to, beside rust-analyzer.
- **Definition, references and hover** through the index daemon's code-index queries, answered by the server.
- **An index.** A worktree with no `Cargo.toml` whose files an installed language with a configured server
  claims has a code index (`LOADING` / `READY`, never `NONE`), at the index daemon and at the session level.

## Not yet

- No code action is run for an extension language, and its server is not told of files changed on disk
  ([backlog](../../dev/todo/2026-10-22-extension-servers-are-not-told-of-tree-changes.md)).
- No `extension.wasm` is run; no grammar or extension is built.

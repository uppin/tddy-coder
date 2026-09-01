//! Resolves Rust intents by driving rust-analyzer as a language server.
//!
//! Extraction is an *assist*, reachable only through `textDocument/codeAction` followed by
//! `codeAction/resolve` — the `ssr` subcommand cannot do it. rust-analyzer names the function it
//! extracts `fun_name` and expects the client to rename it, so the backend performs a second
//! `textDocument/rename` round trip rather than rewriting the identifier itself.
//!
//! Snippet support is deliberately not advertised: with it, the assist embeds `$0` cursor markers
//! that a non-editor client would write straight into the source.

use crate::backends::lsp_bridge::LspClientBridge;
use crate::edit::{
    FileEdit, Position, Range, Resolution, TextEdit, VisibilityChange, WorkspaceEdit,
};
use crate::plan::{Anchor, Reexport, RefactorKind, RefactorOp};
use crate::registry::{Language, LanguageBackend, Workspace};
use crate::{RestructureError, Result};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tddy_lsp::client::LspClient;

/// LSP `SymbolKind::Object` — how rust-analyzer reports an `impl` block. Its members are reached
/// through the type, never through a module path, which is why a seam may move a whole `impl` freely
/// and may not cut one in half.
const SYMBOL_KIND_IMPL: u64 = 19;

/// LSP `SymbolKind::Module` — the only nesting a module path grows through. Verified against
/// rust-analyzer's own `documentSymbol`, which reports an `impl` block as `Object` (19) carrying
/// `Method` (6) children, and an inline `mod` as `Module` (2).
const SYMBOL_KIND_MODULE: u64 = 2;

const SUPPORTED: [RefactorKind; 7] = [
    RefactorKind::ExtractMethod,
    RefactorKind::ExtractVariable,
    RefactorKind::ExtractModule,
    RefactorKind::ExtractModuleToFile,
    RefactorKind::ExtractTrait,
    RefactorKind::InlineMethod,
    RefactorKind::RenameSymbol,
];

/// How to ask rust-analyzer for the assist behind an operation.
#[derive(Clone, Copy)]
struct Assist {
    /// The server's title in full, compared after lowercasing. Matching a substring would let
    /// `Extract Module` also select `Extract module to file`, which is a different assist.
    title: &'static str,
    /// Code action kinds to filter on. Empty means unfiltered, which is what the assists carrying
    /// no LSP kind of their own — the `Generate` family — need in order to be offered at all.
    kinds: &'static [&'static str],
    /// Whether the request addresses a caret rather than the anchor's whole range. The assists that
    /// act on a whole item are offered at its keyword or name, not for a selection spanning it.
    at_caret: bool,
    /// The keyword and name rust-analyzer writes for the symbol it introduces, which the client is
    /// expected to rename. `None` when the assist introduces nothing to name.
    placeholder: Option<Placeholder>,
    /// Whether the assist may edit files other than the anchor's own. The single-document path
    /// keeps that one file, which for an inline is not a missing feature but a silent corruption:
    /// the definition goes and every caller elsewhere keeps calling it.
    multi_file: bool,
    /// Whether the assist writes types that only rust-analyzer's *inference* can supply, rather than
    /// only moving text the syntax tree already carries. Such an assist can be offered before
    /// inference is ready — the shape comes from the tree — and then fills what it does not yet know
    /// with `_`, which is not legal in an item signature.
    needs_inference: bool,
    /// Whether the assist relocates items into a scope of their own. Two things follow, and both
    /// have to be dealt with before the result compiles: the file's `use` declarations stay where
    /// they are, so every name they bound goes unresolved in the new scope; and the path that
    /// reaches the items changes, while rust-analyzer rewrites no reference to them.
    relocates_items: bool,
}

/// One entry of a workspace edit's `documentChanges`, as this executor's edits.
///
/// `created` accumulates the files this same edit brought into existence, because their "original"
/// content is empty rather than something to read from disk.
fn convert_change(
    change: &Value,
    workspace: &Workspace<'_>,
    created: &mut Vec<String>,
) -> Result<Vec<FileEdit>> {
    match change.get("kind").and_then(Value::as_str) {
        Some("create") => {
            let path = relative_path(change.get("uri"), workspace.root)?;
            created.push(path.clone());
            Ok(vec![FileEdit::Create { path }])
        }
        Some(other) => Err(failure(format!("unsupported resource operation `{other}`"))),
        None => {
            let path = relative_path(change.pointer("/textDocument/uri"), workspace.root)?;
            let original = if created.contains(&path) {
                String::new()
            } else {
                workspace.read(&path)?
            };
            let updated = apply_lsp_edit(&original, edits_in(change)?);
            Ok(vec![FileEdit::Change {
                path,
                edits: minimal_edits(&original, &updated),
            }])
        }
    }
}

/// The code action titled `wanted`, compared in full after lowercasing.
///
/// The comparison is exact because the catalog contains titles that are prefixes of one another —
/// `Extract Module` and `Extract module to file` are different assists — and a substring match
/// would silently run whichever the server happened to list first.
fn titled(actions: &Value, wanted: &str) -> Option<Value> {
    actions
        .as_array()?
        .iter()
        .find(|action| {
            action
                .get("title")
                .and_then(Value::as_str)
                .is_some_and(|title| title.to_lowercase() == wanted)
        })
        .cloned()
}

/// What this client tells rust-analyzer it can handle.
///
/// Work-done progress and rust-analyzer's `serverStatus` extension are both requested so the
/// warm-up has something to show and something to stop on; without the first the server sends no
/// `$/progress` at all, and a run that spends two minutes loading a crate graph shows nothing.
///
/// Snippet edits are withheld on purpose: they carry `$0` cursor markers meant for an editor, which
/// a non-interactive client would write straight into the source. Hierarchical document symbols are
/// requested because the flat shape reports a range starting at the item's visibility keyword,
/// which a rename refuses. Semantic tokens are requested for the one type rust-analyzer adds to the
/// standard set — `unresolvedReference`, which is how a moved item's lost names are found — and the
/// declared type list is left empty because the legend comes back from the server either way.
fn client_capabilities() -> Value {
    json!({
        "workspace": {
            "workspaceEdit": {
                "documentChanges": true,
                "resourceOperations": ["create", "rename", "delete"]
            }
        },
        "textDocument": {
            "codeAction": {
                "codeActionLiteralSupport": {
                    "codeActionKind": {
                        "valueSet": [
                            "quickfix",
                            "refactor.extract",
                            "refactor.inline",
                            "refactor.rewrite"
                        ]
                    }
                },
                "resolveSupport": { "properties": ["edit"] },
                "dataSupport": true
            },
            "rename": { "prepareSupport": false },
            "documentSymbol": { "hierarchicalDocumentSymbolSupport": true },
            "semanticTokens": {
                "requests": { "full": true },
                "formats": ["relative"],
                "tokenTypes": [],
                "tokenModifiers": []
            }
        },
        "window": { "workDoneProgress": true },
        "experimental": { "snippetTextEdit": false, "serverStatusNotification": true },
        // Every position this client converts is a byte offset into a Rust source file, which is
        // what `str` indexing wants. Left undeclared, the protocol default is UTF-16 and the server
        // answers in code units — so a line carrying a character outside the BMP resolved to the
        // wrong column, silently and only on that line.
        "general": { "positionEncodings": [BYTE_ENCODING] }
    })
}

/// The position encoding this client asks for and is able to count in.
///
/// UTF-8 means "byte offsets", which is the only unit `offset_of` and `position_at` can agree on
/// without carrying the negotiated encoding through every conversion.
const BYTE_ENCODING: &str = "utf-8";

/// The encoding the server settled on, defaulting the way the specification does.
///
/// A server that cannot speak UTF-8 answers in UTF-16 whatever the client asked for, so this is read
/// back rather than assumed — the alternative is counting bytes against code units, which is the
/// defect this replaces.
fn negotiated_encoding(handshake: &Value) -> &str {
    handshake
        .pointer("/capabilities/positionEncoding")
        .and_then(Value::as_str)
        .unwrap_or("utf-16")
}

/// Settings for the server, sent with the handshake.
///
/// Import granularity is pinned rather than left to whatever the server defaults to, because the
/// import pass adds one `use` per round trip: unpinned, a dozen names from one crate arrive as a
/// dozen separate declarations, and which of them merge depends on the order the names came back in.
/// Asking for crate-level grouping, enforced, makes the result both tidier and the same every run.
fn server_settings() -> Value {
    json!({
        "imports": {
            "granularity": { "group": "crate", "enforce": true },
            "merge": { "glob": false }
        }
    })
}

/// The declaration an assist leaves behind for the client to rename, split so the identifier's
/// offset follows from the keyword's length rather than from re-parsing the declaration.
#[derive(Clone, Copy)]
struct Placeholder {
    keyword: &'static str,
    name: &'static str,
}

impl Placeholder {
    fn declaration(&self) -> String {
        format!("{} {}", self.keyword, self.name)
    }

    /// Offset of the identifier within the declaration.
    fn identifier_offset(&self) -> usize {
        self.keyword.len() + 1
    }
}

fn assist_for(kind: RefactorKind) -> Option<Assist> {
    match kind {
        RefactorKind::ExtractMethod => Some(Assist {
            title: "extract into function",
            kinds: &["refactor.extract"],
            at_caret: false,
            placeholder: Some(Placeholder {
                keyword: "fn",
                name: "fun_name",
            }),
            multi_file: false,
            needs_inference: true,
            relocates_items: false,
        }),
        RefactorKind::ExtractVariable => Some(Assist {
            title: "extract into variable",
            kinds: &["refactor.extract"],
            at_caret: false,
            placeholder: Some(Placeholder {
                keyword: "let",
                name: "var_name",
            }),
            multi_file: false,
            needs_inference: true,
            relocates_items: false,
        }),
        RefactorKind::ExtractModule => Some(Assist {
            title: "extract module",
            kinds: &["refactor.extract"],
            at_caret: false,
            placeholder: Some(Placeholder {
                keyword: "mod",
                name: "modname",
            }),
            multi_file: false,
            needs_inference: false,
            relocates_items: true,
        }),
        RefactorKind::ExtractModuleToFile => Some(Assist {
            title: "extract module to file",
            kinds: &[],
            at_caret: true,
            placeholder: None,
            multi_file: true,
            needs_inference: false,
            relocates_items: false,
        }),
        // The `Generate` family carries no LSP kind, so filtering on one hides it entirely.
        RefactorKind::ExtractTrait => Some(Assist {
            title: "generate trait from impl",
            kinds: &[],
            at_caret: true,
            placeholder: Some(Placeholder {
                keyword: "trait",
                name: "NewTrait",
            }),
            multi_file: false,
            needs_inference: false,
            relocates_items: false,
        }),
        RefactorKind::InlineMethod => Some(Assist {
            title: "inline into all callers",
            kinds: &["refactor.inline"],
            at_caret: true,
            placeholder: None,
            multi_file: true,
            needs_inference: false,
            relocates_items: false,
        }),
        _ => None,
    }
}

/// What the server has said about its own progress while a request was in flight.
///
/// `request` used to drop every message it was not waiting for, which is why a run that spent two
/// minutes loading a crate graph showed nothing at all and then failed claiming the plan was
/// malformed. Folding those messages in costs one match per message and turns the wait into
/// something a developer can watch.
#[derive(Default)]
struct ServerChatter {
    /// The title of each work-done progress token in flight, by token. A title arrives only with
    /// `begin`, so it has to be carried forward to the `report` lines that follow — and per token,
    /// because rust-analyzer runs several phases at once and a single field would attribute one
    /// phase's reports to another's title.
    titles: HashMap<String, String>,
    /// The last line built, which is what the timeout message needs in order to say where the
    /// server got to.
    last: Option<String>,
    /// What was last printed for each token, deduplicated per token for the same reason the titles
    /// are: two phases running at once would otherwise each defeat the other's deduplication.
    shown: HashMap<String, String>,
    /// Whether the server has reported itself quiescent — an extension, so never the only signal.
    quiescent: bool,
}

impl ServerChatter {
    /// Fold one server-sent message in, and return the line worth printing for it.
    ///
    /// A message that answers a request carries no `method`, which is what keeps every result out
    /// of the progress stream without having to know the ids in flight.
    fn absorb(&mut self, message: &Value) -> Option<String> {
        match message.get("method").and_then(Value::as_str)? {
            "$/progress" => self.progress(message.get("params")?),
            "experimental/serverStatus" => {
                self.quiescent = message
                    .get("params")?
                    .get("quiescent")
                    .and_then(Value::as_bool)?;
                None
            }
            _ => None,
        }
    }

    /// Fold one `$/progress` notification in.
    ///
    /// The title arrives only with `begin`, so it is held and reused for the `report` lines that
    /// follow it — without that, a report reads as a bare percentage with nothing to attach it to.
    /// Lines are deduplicated on the phase and its percentage rather than on the whole line, because
    /// the server reports one notification per *file* scanned and each carries a different absolute
    /// path. Printing all of them buries the phases; one line per percent of each phase is the
    /// progress a reader can actually follow. A notification with no percentage — every `begin`, and
    /// the sub-steps of a phase that does not count — falls back to the line itself.
    fn progress(&mut self, params: &Value) -> Option<String> {
        let token = token_key(params.get("token")?);
        let value = params.get("value")?;
        match value.get("kind").and_then(Value::as_str)? {
            "begin" => {
                if let Some(title) = value.get("title").and_then(Value::as_str) {
                    self.titles.insert(token.clone(), title.to_string());
                }
            }
            "end" => {
                self.titles.remove(&token);
                return None;
            }
            _ => {}
        }

        let title = self.titles.get(&token).map(String::as_str);
        let line = progress_line(title, value);
        self.last = Some(line.clone());

        let key = match value.get("percentage").and_then(Value::as_u64) {
            Some(percentage) => percentage.to_string(),
            None => line.clone(),
        };
        if self.shown.get(&token) == Some(&key) {
            return None;
        }
        self.shown.insert(token, key);
        Some(line)
    }
}

/// A progress token as a map key. The specification allows a string or a number.
fn token_key(token: &Value) -> String {
    match token.as_str() {
        Some(text) => text.to_string(),
        None => token.to_string(),
    }
}

/// One progress notification as a line: what the server is doing, where it has got to, and how far.
fn progress_line(title: Option<&str>, value: &Value) -> String {
    let mut line = title.unwrap_or("working").to_string();
    if let Some(message) = value.get("message").and_then(Value::as_str) {
        line.push_str(": ");
        line.push_str(message);
    }
    if let Some(percentage) = value.get("percentage").and_then(Value::as_u64) {
        line.push_str(&format!(" ({percentage}%)"));
    }
    line
}

/// How long the one-time warm-up may spend waiting for the crate graph, when a run does not say.
///
/// Generous on purpose: it is paid once per process, and the alternative to waiting is a refusal
/// that reads as a defect in the plan. A cold `~/.cargo` on a loaded machine is the case this covers.
const WARMUP_BUDGET: Duration = Duration::from_secs(600);

/// How long a wait for name resolution may spend *after* the warm-up has succeeded.
///
/// By then the graph is loaded and the only thing left to wait out is the server catching up with
/// this client's own edits, which is seconds. Keeping this short is the point of the warm-up: these
/// waits are paid per operation, and `survey_moved_items` pays one per moved item.
const SETTLE_BUDGET: Duration = Duration::from_secs(30);

/// rust-analyzer answers `codeAction` with an empty list until it has finished loading the crate
/// graph, so a request that needs the graph is retried at this cadence until it is answered.
const INDEXING_POLL: Duration = Duration::from_secs(2);
const SETTLE_POLL: Duration = Duration::from_millis(200);

/// The semantic-token type rust-analyzer gives an identifier it cannot resolve. It is an extension
/// to the standard legend, and the only way this client learns which names a moved item has lost
/// without waiting on a `cargo check` it would otherwise have to run per pass.
const UNRESOLVED_TOKEN: &str = "unresolvedReference";

/// The prefix of the code action that adds a `use` declaration. Its sibling `Qualify as …` fixes
/// the same diagnostic by rewriting the reference instead, which is not what an extraction wants.
const IMPORT_TITLE: &str = "Import ";

/// How many times the extraction will ask for one more import. Each pass restores a single name and
/// routinely resolves several others that were only unresolved through it, so the count needed is
/// far below this; the bound is here so a server that keeps offering an import that changes nothing
/// stops rather than spins.
const IMPORT_PASSES: usize = 64;

/// LSP `ContentModified`. The server is still catching up with a document change and asks the
/// client to re-issue, which is what the specification prescribes rather than treating it as fatal.
const CONTENT_MODIFIED: i64 = -32801;
const CONTENT_MODIFIED_RETRIES: u32 = 30;

pub struct RustBackend {
    binary: PathBuf,
    cargo_home: PathBuf,
    rustup_home: PathBuf,
    server: Option<Server>,
    /// When set, LSP traffic goes through an existing client instead of a spawned server.
    bridge: Option<LspClientBridge>,
    next_id: u64,
    /// What the running server was launched against, for the indexing-timeout message.
    environment: String,
    /// What the server has said while this client was waiting on an answer.
    chatter: ServerChatter,
    /// How long the one-time warm-up may run before it gives up.
    warmup: Duration,
    /// Where a progress line goes. Every other consequence of an operation travels back to the
    /// caller inside a [`Resolution`], but progress happens *while* a call is in flight and has
    /// nowhere to wait — so it needs a sink rather than a return value. It stays a sink rather than
    /// a `println!` because this library is not the only possible front end: anything that speaks a
    /// protocol on stdout, a persistent server most obviously, would have its stream corrupted by an
    /// engine writing progress into it. Silent by default, and the binary is what makes it visible.
    progress: fn(&str),
    /// Where a diagnostic trace goes. A second sink rather than a level on the first, because they
    /// have different audiences: progress is for the person waiting, and this is for whoever is
    /// working out why a seam behaved as it did. Silent unless the front end installs one.
    ///
    /// It exists because the alternative is guessing. The `impl`-sibling survey returning nothing was
    /// settled in one run by printing what the traversal saw, after two rounds of reasoning about it
    /// had reached the wrong answer.
    trace: fn(&str),
    /// Whether the crate graph has been observed loaded. Set once, and never unset: the graph does
    /// not unload, so every later wait is a settle rather than an index.
    indexed: bool,
    /// Index of `unresolvedReference` in the server's semantic-token legend, read from the
    /// initialize handshake. The legend is per-server, so it is not a constant to hard-code.
    unresolved_token: Option<u32>,
    /// The version last sent for any open document.
    ///
    /// One counter for every document rather than one each: the protocol only asks that a document's
    /// versions increase, and a single monotonic counter satisfies that for all of them. It lives here
    /// because a chained assist has to send changes *after* whatever the import passes sent, and
    /// threading the number through every one of them by hand is how that goes wrong.
    doc_version: u64,
    /// Module names earlier operations in this plan have already introduced, paired with the file
    /// that holds them.
    ///
    /// A collision with a declaration that was always in the file is lexical and needs nothing
    /// remembered; a collision with a module an earlier seam invented is only visible to something
    /// walking the whole plan, and it is `E0428` just the same. Keyed by file because `E0428` is a
    /// collision *within one namespace* — two files may each declare `mod shared` perfectly legally,
    /// and reporting that as a collision would refuse a plan Rust accepts.
    claimed: Vec<(String, String)>,
}

/// The default progress sink: a library that was not asked to report says nothing.
fn discard(_line: &str) {}

/// Drop `use` declarations left binding nothing at all.
///
/// Moving items out of a file leaves the parent importing what they needed, and where every name in a
/// grouped import moved the assist hollows the group out rather than removing the line —
/// `use std::sync::{};`. An empty group binds nothing, ever, so this needs no evidence from the server
/// and cannot be wrong: it is the one part of the unused-import tail that is decidable by looking.
///
/// Deliberately only the empty group. A `use` that resolves is left alone however unused it looks,
/// because dropping a trait import breaks method resolution invisibly — the same reason
/// [`RustBackend::prune_assist_imports`] is narrow.
fn without_hollow_imports(text: &str) -> String {
    let kept: Vec<&str> = text
        .split('\n')
        .filter(|line| !binds_nothing(line))
        .collect();
    kept.join("\n")
}

/// Whether a line is a `use` whose brace group is empty.
fn binds_nothing(line: &str) -> bool {
    let body = line.trim();
    let body = body.strip_prefix("pub ").unwrap_or(body);
    let Some(rest) = body.strip_prefix("use ") else {
        return false;
    };
    let Some(inner) = rest.strip_suffix(';') else {
        return false;
    };
    let Some(open) = inner.find('{') else {
        return false;
    };

    inner.ends_with('}') && inner[open + 1..inner.len() - 1].trim().is_empty()
}

struct Server {
    process: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

/// One line naming everything that decides how rust-analyzer resolves std and dependencies.
///
/// A stall at `discovering sysroot` looks the same whether the toolchain was pinned, whether
/// cargo/rustc are real binaries or rustup proxies, and whether rust-src is present. This is
/// what tells them apart in a CI log.
fn describe_server_environment(binary: &Path, toolchain: &str, toolchain_bin: &Path) -> String {
    let present = |name: &str| {
        if toolchain_bin.join(name).exists() {
            "real"
        } else {
            "missing"
        }
    };
    let rust_src = toolchain_bin
        .parent()
        .map(|prefix| prefix.join("lib/rustlib/src/rust/library"))
        .is_some_and(|path| path.exists());
    format!(
        "server={}; RUSTUP_TOOLCHAIN={toolchain}; cargo={}; rustc={}; rust-src={}",
        binary.display(),
        present("cargo"),
        present("rustc"),
        if rust_src { "present" } else { "absent" },
    )
}

fn default_toolchain_name(rustup_home: &Path) -> Option<String> {
    std::fs::read_to_string(rustup_home.join("settings.toml"))
        .ok()
        .and_then(|contents| {
            contents.lines().find_map(|line| {
                let rest = line.strip_prefix("default_toolchain")?;
                rest.split('"').nth(1).map(str::to_string)
            })
        })
}

impl RustBackend {
    pub fn new(
        binary: impl Into<PathBuf>,
        cargo_home: impl Into<PathBuf>,
        rustup_home: impl Into<PathBuf>,
    ) -> Self {
        Self {
            binary: binary.into(),
            cargo_home: cargo_home.into(),
            rustup_home: rustup_home.into(),
            server: None,
            bridge: None,
            next_id: 0,
            environment: String::from("<server not started>"),
            chatter: ServerChatter::default(),
            warmup: WARMUP_BUDGET,
            progress: discard,
            trace: discard,
            indexed: false,
            unresolved_token: None,
            doc_version: 1,
            claimed: Vec::new(),
        }
    }

    /// Give the warm-up a budget other than the default.
    pub fn with_indexing_budget(mut self, seconds: u64) -> Self {
        self.warmup = Duration::from_secs(seconds);
        self
    }

    /// Send progress somewhere. Without this the indexing wait is silent, which is the state the
    /// field report spent two hours in.
    pub fn with_progress(mut self, sink: fn(&str)) -> Self {
        self.progress = sink;
        self
    }

    /// Install a diagnostic trace. Silent by default, and never on stdout by this library's choice.
    pub fn with_trace(mut self, sink: fn(&str)) -> Self {
        self.trace = sink;
        self
    }

    /// Attach to an already-initialized rust-analyzer session from `tddy-lsp`.
    ///
    /// No child process is spawned; [`LspClientBridge`] forwards requests through the shared
    /// client via `Handle::current().block_on`.
    pub fn from_lsp_client(
        client: Arc<LspClient>,
        indexing_budget: Option<u64>,
        progress: fn(&str),
    ) -> Self {
        let mut backend = Self {
            binary: PathBuf::new(),
            cargo_home: PathBuf::new(),
            rustup_home: PathBuf::new(),
            server: None,
            bridge: Some(LspClientBridge::new(client)),
            next_id: 0,
            environment: String::from("external tddy-lsp client"),
            chatter: ServerChatter::default(),
            warmup: WARMUP_BUDGET,
            progress,
            trace: discard,
            indexed: false,
            unresolved_token: None,
            doc_version: 1,
            claimed: Vec::new(),
        };
        if let Some(seconds) = indexing_budget {
            backend.warmup = Duration::from_secs(seconds);
        }
        backend
    }

    fn take_id(&mut self) -> u64 {
        self.next_id += 1;
        self.next_id
    }

    /// Start rust-analyzer and complete the initialize handshake, once per run.
    fn start(&mut self, root: &Path) -> Result<()> {
        if self.bridge.is_some() {
            return Ok(());
        }
        if self.server.is_some() {
            return Ok(());
        }

        // Pinning is mandatory, not best-effort. Skipping it let rust-analyzer's cargo and
        // rustc fall through the rustup proxy, which walks up to the repo-root
        // rust-toolchain.toml — non-empty `components`/`targets`, so the proxy channel-syncs
        // over the network. That is a candidate for the 600s stall at `discovering sysroot`
        // (Falcon e34fef02), and it fails silently, which is worse than failing.
        let toolchain = default_toolchain_name(&self.rustup_home).ok_or_else(|| {
            failure(format!(
                "could not read default_toolchain from {}/settings.toml — refusing to start \
                 rust-analyzer unpinned, because the rustup proxy would channel-sync the repo \
                 rust-toolchain.toml overlay instead",
                self.rustup_home.display()
            ))
        })?;
        let toolchain_bin = self
            .rustup_home
            .join("toolchains")
            .join(&toolchain)
            .join("bin");

        let mut command = Command::new(&self.binary);
        command
            .current_dir(root)
            .env("CARGO_HOME", &self.cargo_home)
            .env("RUSTUP_HOME", &self.rustup_home)
            .env("RUSTUP_TOOLCHAIN", &toolchain)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());

        // Point rust-analyzer straight at the real binaries so sysroot discovery and
        // `cargo metadata` never go through a proxy at all.
        for (key, name) in [("CARGO", "cargo"), ("RUSTC", "rustc")] {
            let real = toolchain_bin.join(name);
            if real.exists() {
                command.env(key, real);
            }
        }
        self.environment = describe_server_environment(&self.binary, &toolchain, &toolchain_bin);
        let mut process = command
            .spawn()
            .map_err(|error| failure(format!("could not start rust-analyzer: {error}")))?;

        let stdin = process.stdin.take().expect("stdin was piped");
        let stdout = BufReader::new(process.stdout.take().expect("stdout was piped"));
        self.server = Some(Server {
            process,
            stdin,
            stdout,
        });

        let id = self.take_id();
        let handshake = self.request(
            id,
            "initialize",
            json!({
                "processId": std::process::id(),
                "rootUri": uri_of(root),
                "capabilities": client_capabilities(),
                "initializationOptions": server_settings()
            }),
        )?;
        let encoding = negotiated_encoding(&handshake);
        if encoding != BYTE_ENCODING {
            return Err(failure(format!(
                "rust-analyzer settled on `{encoding}` positions, and this client counts \
                 `{BYTE_ENCODING}` — every column it converted would be wrong on any line carrying a \
                 character outside the BMP. Refusing rather than resolving anchors against the wrong \
                 unit."
            )));
        }

        self.unresolved_token = token_type_index(&handshake, UNRESOLVED_TOKEN);
        self.notify("initialized", json!({}))
    }

    fn request(&mut self, id: u64, method: &str, params: Value) -> Result<Value> {
        if let Some(bridge) = &self.bridge {
            return bridge.request(method, params);
        }
        self.send(json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }))?;
        loop {
            let message = self.receive()?;
            if let Some(line) = self.chatter.absorb(&message) {
                (self.progress)(&line);
            }
            // A message carrying both an id and a method is the server asking this client for
            // something — in practice only `window/workDoneProgress/create`, which has to be
            // answered or the server stops reporting progress through that token.
            if let (Some(server_id), Some(_)) = (
                message.get("id").and_then(Value::as_u64),
                message.get("method"),
            ) {
                self.send(json!({ "jsonrpc": "2.0", "id": server_id, "result": null }))?;
                continue;
            }
            if message.get("id").and_then(Value::as_u64) == Some(id) {
                if let Some(error) = message.get("error") {
                    return Err(match error.get("code").and_then(Value::as_i64) {
                        Some(CONTENT_MODIFIED) => RestructureError::ServerCatchingUp,
                        _ => failure(format!("rust-analyzer: {error}")),
                    });
                }
                return Ok(message.get("result").cloned().unwrap_or(Value::Null));
            }
        }
    }

    /// Issue a request, re-sending while the server reports it is still catching up.
    fn request_settled(&mut self, method: &str, params: Value) -> Result<Value> {
        for _ in 0..CONTENT_MODIFIED_RETRIES {
            let id = self.take_id();
            match self.request(id, method, params.clone()) {
                Err(RestructureError::ServerCatchingUp) => std::thread::sleep(SETTLE_POLL),
                outcome => return outcome,
            }
        }
        Err(failure(format!(
            "rust-analyzer never settled enough to answer {method}"
        )))
    }

    fn notify(&mut self, method: &str, params: Value) -> Result<()> {
        if let Some(bridge) = &self.bridge {
            return bridge.notify(method, params);
        }
        self.send(json!({ "jsonrpc": "2.0", "method": method, "params": params }))
    }

    fn send(&mut self, message: Value) -> Result<()> {
        let server = self
            .server
            .as_mut()
            .ok_or_else(|| failure("rust-analyzer is not running"))?;
        let body = serde_json::to_vec(&message).map_err(|error| failure(error.to_string()))?;
        write!(server.stdin, "Content-Length: {}\r\n\r\n", body.len())
            .map_err(|error| failure(error.to_string()))?;
        server
            .stdin
            .write_all(&body)
            .map_err(|error| failure(error.to_string()))?;
        server
            .stdin
            .flush()
            .map_err(|error| failure(error.to_string()))?;
        Ok(())
    }

    fn receive(&mut self) -> Result<Value> {
        let server = self
            .server
            .as_mut()
            .ok_or_else(|| failure("rust-analyzer is not running"))?;

        let mut length = 0usize;
        loop {
            let mut header = String::new();
            if server
                .stdout
                .read_line(&mut header)
                .map_err(|e| failure(e.to_string()))?
                == 0
            {
                return Err(failure("rust-analyzer closed the connection"));
            }
            let header = header.trim_end();
            if header.is_empty() {
                break;
            }
            if let Some(value) = header.strip_prefix("Content-Length: ") {
                length = value
                    .parse()
                    .map_err(|_| failure("unreadable Content-Length"))?;
            }
        }

        let mut body = vec![0u8; length];
        server
            .stdout
            .read_exact(&mut body)
            .map_err(|error| failure(error.to_string()))?;
        serde_json::from_slice(&body).map_err(|error| failure(error.to_string()))
    }

    fn did_open(&mut self, uri: &str, text: &str) -> Result<()> {
        self.notify(
            "textDocument/didOpen",
            json!({ "textDocument": { "uri": uri, "languageId": "rust", "version": 1, "text": text } }),
        )
    }

    fn did_change(&mut self, uri: &str, text: &str) -> Result<()> {
        self.doc_version += 1;
        let version = self.doc_version;
        self.notify(
            "textDocument/didChange",
            json!({
                "textDocument": { "uri": uri, "version": version },
                "contentChanges": [{ "text": text }]
            }),
        )
    }

    /// Ask for a named assist, retrying while the crate graph is still loading.
    fn assist(&mut self, uri: &str, range: Range, kind: RefactorKind) -> Result<Value> {
        let assist = assist_for(kind)
            .ok_or_else(|| failure(format!("no rust-analyzer assist maps to {kind:?}")))?;
        let wanted = assist.title;
        let target = if assist.at_caret {
            Range {
                start: range.start,
                end: range.start,
            }
        } else {
            range
        };
        let deadline = Instant::now() + self.resolution_budget();
        loop {
            let id = self.take_id();
            let actions = match self.request(
                id,
                "textDocument/codeAction",
                json!({
                    "textDocument": { "uri": uri },
                    "range": lsp_range(target),
                    "context": context_for(assist.kinds)
                }),
            ) {
                Ok(actions) => actions,
                // Still loading the crate graph; that is what this loop is waiting out.
                Err(RestructureError::ServerCatchingUp) => Value::Null,
                Err(error) => return Err(error),
            };

            if let Some(action) = titled(&actions, wanted) {
                return Ok(action);
            }
            if Instant::now() >= deadline {
                return Err(failure(format!(
                    "rust-analyzer offers no \"{wanted}\" assist for the given range"
                )));
            }
            std::thread::sleep(INDEXING_POLL);
        }
    }
}

impl Drop for RustBackend {
    fn drop(&mut self) {
        if let Some(mut server) = self.server.take() {
            drop(server.stdin);
            let _ = server.process.wait();
        }
    }
}

impl LanguageBackend for RustBackend {
    fn language(&self) -> Language {
        Language::Rust
    }

    fn handles_extension(&self, extension: &str) -> bool {
        extension == "rs"
    }

    fn supports(&self, kind: RefactorKind) -> bool {
        SUPPORTED.contains(&kind)
    }

    /// The refusals that read only text, run over one operation without starting a server.
    ///
    /// These same two checks already run per operation inside `resolve`, before the server starts —
    /// but there they stop the run at the first one, which is right for applying and wrong for
    /// reporting. Here they are collected, and the module names the plan has already claimed are
    /// carried forward, so a seam colliding with an earlier seam's new module is caught as well as one
    /// colliding with a declaration that was always there.
    fn check(&mut self, op: &RefactorOp, workspace: &Workspace<'_>) -> Result<Vec<String>> {
        let Anchor::Range { start, end, .. } = &op.anchor else {
            return Ok(Vec::new());
        };

        let text = workspace.read(op.anchor.file())?;
        let planned = Range {
            start: *start,
            end: *end,
        };
        let mut findings = Vec::new();

        if let Err(refusal) = refuse_split_attribute_paths(&text, planned) {
            findings.push(refusal.to_string());
        }

        if op.op == RefactorKind::ExtractModule {
            if let Some(name) = op.name.as_deref() {
                let file = op.anchor.file();
                if let Err(refusal) = refuse_module_name_taken(&text, name, planned) {
                    findings.push(refusal.to_string());
                } else if self
                    .claimed
                    .iter()
                    .any(|(held, claimed)| held == file && claimed == name)
                {
                    findings.push(format!(
                        "`{name}` is already claimed by an earlier operation in this plan, in the \
                         same file. Two declarations of the name is `E0428`, and the second assist \
                         writes it without complaint. Give the module a different name."
                    ));
                }
                self.claimed.push((file.to_string(), name.to_string()));
            }
        }

        Ok(findings)
    }

    /// Where a named, adjacent run of items begins and ends, with the trivia attached to the first.
    ///
    /// The outline comes from the server, so the extents are the ones the assist itself will see. Two
    /// things are refused rather than approximated: an item the file does not define, which is a
    /// mistake in the request and not an empty range; and items that are not adjacent, because a seam
    /// is one contiguous range and the span between two distant items would silently carry everything
    /// in between.
    fn anchor_for(
        &mut self,
        file: &str,
        items: &[String],
        workspace: &Workspace<'_>,
    ) -> Result<Range> {
        if items.is_empty() {
            return Err(failure("`--items` named nothing to cover"));
        }

        let text = workspace.read(file)?;
        let uri = uri_of(&workspace.root.join(file));

        self.start(workspace.root)?;
        self.did_open(&uri, &text)?;
        self.ensure_indexed(&uri)?;

        let outline = self.module_outline(&uri)?;
        let places = places_of(&outline, items, file)?;
        refuse_non_adjacent(&outline, &places)?;

        let first = &outline[places[0]];
        let last = &outline[places[places.len() - 1]];

        Ok(Range {
            start: Position {
                line: attached_trivia_starts_at(&text, first.start_line + 1),
                col: 1,
            },
            end: Position {
                line: last.end_line + 1,
                col: last.end_column + 1,
            },
        })
    }

    fn resolve(&mut self, op: &RefactorOp, workspace: &Workspace<'_>) -> Result<Resolution> {
        if !self.supports(op.op) {
            return Err(RestructureError::UnsupportedOp {
                backend: "Rust".to_string(),
                op: format!("{:?}", op.op),
            });
        }

        let relative = op.anchor.file().to_string();
        let absolute = workspace.root.join(&relative);
        let uri = uri_of(&absolute);
        let original = workspace.read(&relative)?;

        // Both of these are answered by reading the text, so they are answered before a server
        // exists: an operation that cannot be honoured should not first cost an index.
        if let Anchor::Range { start, end, .. } = &op.anchor {
            let planned = Range {
                start: *start,
                end: *end,
            };
            refuse_split_attribute_paths(&original, planned)?;
            if op.op == RefactorKind::ExtractModule {
                if let Some(name) = op.name.as_deref() {
                    refuse_module_name_taken(&original, name, planned)?;
                }
            }
        }

        self.start(workspace.root)?;
        self.did_open(&uri, &original)?;
        self.ensure_indexed(&uri)?;

        // An assist that reaches past the anchor's own file produces its edits directly rather
        // than through the single-document path the in-place assists share.
        if assist_for(op.op).is_some_and(|assist| assist.multi_file) {
            return Ok(Resolution::of(self.multi_file_assist(&uri, workspace, op)?));
        }

        let (final_text, report, notes) = match op.op {
            RefactorKind::RenameSymbol => (
                self.rename_symbol(&uri, &original, op)?,
                Vec::new(),
                Vec::new(),
            ),
            _ => self.assisted_edit(&uri, &original, op)?,
        };

        Ok(Resolution {
            edit: self.edit_for(
                op,
                Produced {
                    uri: &uri,
                    relative: &relative,
                    original: &original,
                    text: &final_text,
                },
                workspace,
            )?,
            report,
            notes,
        })
    }
}

impl RustBackend {
    /// Run an assist that introduces a new symbol, then give that symbol its real name.
    fn assisted_edit(
        &mut self,
        uri: &str,
        original: &str,
        op: &RefactorOp,
    ) -> Result<(String, Vec<VisibilityChange>, Vec<String>)> {
        let range = self.anchor_range(uri, op)?;
        let relocates = assist_for(op.op).is_some_and(|assist| assist.relocates_items);
        let reexport = op.reexport.unwrap_or(Reexport::None);

        // An assist whose output embeds inferred types has to wait for inference, not merely for the
        // assist to be offered — those are different readiness signals, and the second arrives first.
        // A symbol anchor already waits inside `anchor_range`; a range anchor has nothing to wait on
        // there, because there is no symbol to resolve.
        if assist_for(op.op).is_some_and(|assist| assist.needs_inference) {
            let start = lsp_range(range)["start"].clone();
            self.wait_until_resolved(uri, &start)?;
        }

        let moved = if relocates {
            self.survey_moved_items(uri, original, range)?
        } else {
            Vec::new()
        };

        // A facade leaves the old path resolving through the parent, so a reference elsewhere is no
        // longer stranded and there is nothing here to refuse.
        if relocates && reexport == Reexport::None {
            refuse_stranded(&moved)?;
        }

        // Before the assist, not after it. This same seam is caught today only once rust-analyzer has
        // produced the extraction and the rename has failed to reach the call — a full index spent
        // learning what the reference survey already knew. No facade can help here: a re-export makes
        // a module path resolve, and the call that breaks is not written as one.
        let impl_members = if relocates {
            self.survey_impl_members(uri, original, range)?
        } else {
            Vec::new()
        };
        refuse_impl_sibling_references(&impl_members)?;

        let extracted = self.extract(uri, original, range, op.op)?;
        let name = op
            .name
            .clone()
            .ok_or_else(|| failure("the operation needs a name"))?;
        let placeholder = assist_for(op.op)
            .and_then(|assist| assist.placeholder)
            .ok_or_else(|| failure(format!("{:?} introduces nothing to name", op.op)))?;

        let named = self.rename_placeholder(uri, &extracted, placeholder, &name)?;
        refuse_residual_placeholder(original, &named, placeholder.name)?;
        refuse_inferred_placeholder(&named, &format!("{} {name}", placeholder.keyword))?;

        if !relocates {
            return Ok((named, Vec::new(), Vec::new()));
        }

        // Versions 1 and 2 belong to the open and to the rename above; both import phases send
        // more, so the counter runs across them rather than restarting.
        let pruned = self.prune_assist_imports(uri, &named, &name)?;
        let imported = self.restore_imports(uri, &pruned, &name)?;
        let (preserved, mut report) = restore_visibility(&imported, &name, &moved)?;

        // The widenings the pass above cannot see, because the survey feeding it stops above an
        // `impl`. Read off the text the assist actually produced, so a member it left alone is not
        // reported as though it had moved.
        let relocated: Vec<String> = preserved.split('\n').map(str::to_string).collect();
        report.extend(impl_widenings(
            &relocated,
            &module_bounds(&relocated, &name)?,
            &impl_members,
        ));
        let facade = facade_lines(&name, &moved, reexport)?;
        let notes = empty_facade_note(&name, &facade, reexport)
            .into_iter()
            .collect();

        Ok((
            without_hollow_imports(&with_facade(&preserved, &name, &facade)?),
            report,
            notes,
        ))
    }

    /// What the extraction needs to know about every path-reached item the range would relocate.
    ///
    /// One pass over the server answers all three questions the operation has: whether a reference in
    /// another file would be stranded, whether anything outside the range reaches the item at all
    /// (which decides both what a named facade re-exports and whose visibility may be put back), and
    /// what visibility the item was written with.
    fn survey_moved_items(
        &mut self,
        uri: &str,
        text: &str,
        range: Range,
    ) -> Result<Vec<MovedItem>> {
        let symbols = self.request_settled(
            "textDocument/documentSymbol",
            json!({ "textDocument": { "uri": uri } }),
        )?;

        let mut items = Vec::new();

        for found in path_reached_within(&symbols, range) {
            let reach = self.reach_of(uri, &found.position, range)?;
            items.push(MovedItem {
                visibility: visibility_at(text, &found.position),
                name: found.name,
                within: found.within,
                stranded_in: reach.stranded_in,
                reached_from_outside: reach.from_outside,
                referenced_in_impl_at: Vec::new(),
            });
        }

        Ok(items)
    }

    /// The file's module-level items, in the order they appear.
    fn module_outline(&mut self, uri: &str) -> Result<Vec<OutlineItem>> {
        let symbols = self.request_settled(
            "textDocument/documentSymbol",
            json!({ "textDocument": { "uri": uri } }),
        )?;

        let mut outline: Vec<OutlineItem> = symbols
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(OutlineItem::read)
            .collect();
        outline.sort_by_key(|item| item.start_line);
        Ok(outline)
    }

    /// The members of an `impl` the seam cuts through rather than around, with the lines their
    /// remaining siblings still reference them from.
    ///
    /// Separate from [`Self::survey_moved_items`] on purpose. That survey feeds the facade and the
    /// visibility pass, and both are correct as they stand — an `impl` member belongs in neither,
    /// because no module path names it. Folding these items into that list would change what a named
    /// facade covers and what `restore_visibility` narrows, for a question that has nothing to do
    /// with either.
    ///
    /// Costs one `documentSymbol` and nothing more when the seam cuts no `impl`, which is the usual
    /// case; the per-item reference waits are paid only for members that are actually at risk.
    fn survey_impl_members(
        &mut self,
        uri: &str,
        text: &str,
        range: Range,
    ) -> Result<Vec<MovedItem>> {
        let symbols = self.request_settled(
            "textDocument/documentSymbol",
            json!({ "textDocument": { "uri": uri } }),
        )?;

        let cut = impls_cut_through(&symbols, range);
        let relocated = items_relocated_within(&symbols, range);
        (self.trace)(&seam_trace(range, &cut, &relocated));
        let mut items = Vec::new();

        for found in relocated {
            let Some(holder) = found.within.last() else {
                continue;
            };
            if declares(holder) != Some(Block::Impl) {
                continue;
            }

            // References are only asked about where the answer can change the outcome. A member of an
            // `impl` that moves whole is reached through its type and cannot be stranded, so paying a
            // resolution wait per member would buy nothing — and `survey_moved_items` paying one per
            // item is already the expensive part of a seam.
            let reach = if cut.contains(holder) {
                self.reach_of(uri, &found.position, range)?
            } else {
                Reach::default()
            };

            items.push(MovedItem {
                visibility: visibility_at(text, &found.position),
                name: found.name,
                within: found.within,
                // Carried rather than dropped, though nothing weighs it: an inherent method is found
                // through its type, so relocating the `impl` into a submodule — private or not —
                // leaves every caller resolving, in this crate and in any other. That is verified,
                // not assumed: a `pub` method in a private module builds from a separate crate. It is
                // kept here so the value the survey computed is visible rather than silently thrown
                // away, which reads as a defect even where it is not one.
                stranded_in: reach.stranded_in,
                reached_from_outside: reach.from_outside,
                referenced_in_impl_at: reach.in_file_outside_at,
            });
        }

        Ok(items)
    }

    /// Where the references to one item sit, relative to the range about to be relocated.
    fn reach_of(&mut self, uri: &str, position: &Value, range: Range) -> Result<Reach> {
        // References answer empty rather than pending while the crate graph is still loading, so an
        // empty answer is only worth believing once the position resolves at all.
        self.wait_until_resolved(uri, position)?;

        let references = self.request_settled(
            "textDocument/references",
            json!({
                "textDocument": { "uri": uri },
                "position": position,
                "context": { "includeDeclaration": false }
            }),
        )?;

        let mut reach = Reach::default();

        for reference in references.as_array().into_iter().flatten() {
            let referrer = reference
                .get("uri")
                .and_then(Value::as_str)
                .ok_or_else(|| failure("a reference carries no uri"))?;

            if referrer != uri {
                let file = path_of(referrer)?.display().to_string();
                if !reach.stranded_in.contains(&file) {
                    reach.stranded_in.push(file);
                }
                reach.from_outside = true;
                continue;
            }

            // A reference in this same file still counts as outside when it sits beyond the range
            // being relocated — `tier` goes on calling `clamp` from where it stays.
            let point = LspPoint::read(reference.pointer("/range/start"))?;
            let line = point.line as u32 + 1;
            if line < range.start.line || line > range.end.line {
                reach.from_outside = true;
                if !reach.in_file_outside_at.contains(&line) {
                    reach.in_file_outside_at.push(line);
                }
            }
        }

        Ok(reach)
    }

    /// Import every name the relocated items lost, until the server reports none left to import.
    ///
    /// Extracting a module moves items away from the `use` declarations that gave their references
    /// meaning: the declarations stay in the parent and the names go unresolved in the new scope.
    /// rust-analyzer will not carry them across, but it will say which names it cannot resolve and
    /// what would resolve each one — so every path written here is still the server's own.
    ///
    /// One import per pass. Each inserts a `use` line that moves everything below it, and a name
    /// that looked unimportable often becomes resolvable once the name it hung off is restored.
    ///
    /// Every import is *verified* before it is kept: the trial text goes back to the server and the
    /// name it was meant to resolve has to stop being unresolved. rust-analyzer offers imports that
    /// resolve nothing — one for an inherent associated function, one naming a path two module levels
    /// too high — and the difference between a good and a useless offer is not readable from its
    /// title. Trusting the title wrote four `use` lines that did not compile across one real
    /// restructure, in a run that reported success.
    fn restore_imports(&mut self, uri: &str, extracted: &str, module: &str) -> Result<String> {
        let mut text = extracted.to_string();
        // Names every offered path failed. Re-asking one would be offered the same useless import
        // again, and every pass would insert another copy of it.
        let mut unimportable: Vec<String> = Vec::new();

        for _ in 0..IMPORT_PASSES {
            self.did_change(uri, &text)?;

            match self.next_import(uri, &text, module, &mut unimportable)? {
                Some(imported) => text = imported,
                None => return Ok(text),
            }
        }

        Err(failure(format!(
            "rust-analyzer was still offering imports after {IMPORT_PASSES} passes"
        )))
    }

    /// Drop the `use` lines the assist wrote that cannot be an import the move lost.
    ///
    /// rust-analyzer's `extract_module` writes the new module's imports itself, and this runs before
    /// [`Self::restore_imports`] because some of what it wrote resolves nothing: `use
    /// super::new_with_config;` for a constructor reached as `Type::new_with_config`, where no `use`
    /// binds an associated item; `use super::super::GLOBAL_CONTEXT_MANAGER;`, a path one module level
    /// too high; and a bare `use global_context_api;` beside a grouped import of the same module,
    /// which is `E0252` however well either path reads. Restoration could never answer for these —
    /// they are in the text before it starts — and one real restructure landed five compile errors
    /// from them in a run that reported success.
    ///
    /// Both kinds are removed on the server's own evidence rather than by reading the path: a name
    /// the server reports unresolved *on the `use` line that binds it* is an import that does
    /// nothing. Only single-name lines are ever dropped, so a group carrying other names is never
    /// touched, and a line that resolves is left alone however unused it looks — dropping a trait
    /// import would silently break method resolution.
    fn prune_assist_imports(&mut self, uri: &str, extracted: &str, module: &str) -> Result<String> {
        self.did_change(uri, extracted)?;
        let unresolved = self.unresolved_names(uri, extracted)?;

        let source: Vec<String> = extracted.split('\n').map(str::to_string).collect();
        let block = module_bounds(&source, module)?;

        Ok(without_dead_imports(&source, &block, &unresolved).join("\n"))
    }

    /// The text with one more import restored, or `None` once no name is left to import.
    ///
    /// A name with no import offered is skipped rather than refused: most of them are methods and
    /// fields that are unresolved only because their receiver's type is, and they come back on
    /// their own once it does. What is left when no import remains is for the compiler to judge.
    fn next_import(
        &mut self,
        uri: &str,
        text: &str,
        module: &str,
        unimportable: &mut Vec<String>,
    ) -> Result<Option<String>> {
        let mut asked: Vec<String> = Vec::new();
        let unresolved = self.unresolved_names(uri, text)?;

        for name in &unresolved {
            // One import serves every occurrence of a name, and a name that offered none here will
            // not offer one at its next occurrence either.
            if asked.contains(&name.text) || unimportable.contains(&name.text) {
                continue;
            }
            asked.push(name.text.clone());

            // A name reached through a qualifier is an associated item or a field, and no `use`
            // binds either. rust-analyzer offers one anyway — `use super::new_with_config;` for a
            // constructor called as `NativePDFContextManager::new_with_config` — and that import
            // resolves nothing while looking exactly like a good one.
            if reached_through_qualifier(text, &name.position) {
                unimportable.push(name.text.clone());
                continue;
            }

            // Already bound in this module and still unresolved: the binding that exists is the
            // broken one, and a second is `E0252` however well its path reads.
            if already_bound(text, module, &name.text)? {
                unimportable.push(name.text.clone());
                continue;
            }

            let actions = self.request_settled(
                "textDocument/codeAction",
                json!({
                    "textDocument": { "uri": uri },
                    "range": { "start": name.position, "end": name.position },
                    "context": { "diagnostics": [], "only": ["quickfix"] }
                }),
            )?;

            let offered: Vec<String> = actions
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|action| action.get("title").and_then(Value::as_str))
                .filter(|title| title.starts_with(IMPORT_TITLE))
                .map(str::to_string)
                .collect();

            if offered.is_empty() {
                continue;
            }

            let ordered = import_order(text, &offered).ok_or_else(|| {
                failure(format!(
                    "`{}` could be imported {} ways and neither rust-analyzer nor this file's own \
                     imports say which the moved code meant: {}",
                    name.text,
                    offered.len(),
                    offered.join(", ")
                ))
            })?;

            // How many occurrences the import has to account for. Counted rather than asked as a
            // yes/no, because one name is routinely unresolved in several places and only the
            // occurrence this import was offered for is the one it can answer for.
            let before = occurrences_of(&unresolved, &name.text);

            for title in &ordered {
                let action = titled(&actions, &title.to_lowercase())
                    .ok_or_else(|| failure("the import offered could not be read back"))?;
                let resolved = self.request_settled("codeAction/resolve", action)?;
                let trial = apply_lsp_edit(text, edits_for(&resolved, uri)?);

                self.did_change(uri, &trial)?;

                let after = occurrences_of(&self.unresolved_names(uri, &trial)?, &name.text);
                if after < before {
                    return Ok(Some(trial));
                }
            }

            // Every path the server offered leaves the name unresolved. Writing one anyway is how a
            // successful run lands source that does not compile, so the operation says which name it
            // could not import and what it tried.
            return Err(failure(format!(
                "no import rust-analyzer offered for `{}` left fewer of its {} unresolved \
                 occurrence(s) — tried {}. Writing one anyway is how a run reports success over a \
                 `use` that resolves nothing.",
                name.text,
                before,
                ordered.join(", ")
            )));
        }

        Ok(None)
    }

    /// Every identifier in the open document that the server cannot resolve, in source order.
    fn unresolved_names(&mut self, uri: &str, text: &str) -> Result<Vec<UnresolvedName>> {
        let wanted = self.unresolved_token.ok_or_else(|| {
            failure(format!(
                "rust-analyzer's semantic token legend has no `{UNRESOLVED_TOKEN}`, so the names an \
                 extraction loses cannot be found"
            ))
        })?;

        let tokens = self.request_settled(
            "textDocument/semanticTokens/full",
            json!({ "textDocument": { "uri": uri } }),
        )?;

        Ok(unresolved_in(&tokens, wanted, text))
    }

    /// Run an assist whose edits may land in more than one file, and carry all of them through.
    ///
    /// Moving a module to a file answers with a file creation, the module body destined for it, and
    /// the edit that turns `mod name { … }` into `mod name;`; inlining answers with one edit per
    /// caller wherever the callers live. Both are engine-authored in full, down to the new file's
    /// name, so the whole `documentChanges` list is converted rather than the anchor's entry alone.
    /// Move the module the extraction just wrote into a file of its own, without the intermediate
    /// state ever reaching disk.
    ///
    /// The parent's edit is computed against the text that *was* on disk, not against the grouped text
    /// this operation produced along the way — so one journal entry describes the whole operation and
    /// the ledger sees one edit rather than two, which is what lets the rest of a plan keep addressing
    /// coordinates in the snapshot.
    fn chain_module_to_file(
        &mut self,
        produced: Produced<'_>,
        module: &str,
        workspace: &Workspace<'_>,
    ) -> Result<Vec<FileEdit>> {
        self.did_change(produced.uri, produced.text)?;

        let caret = caret_at_module(produced.text, module)?;

        // An assist that introduces a top-level item leaves the server rebuilding the module tree, and
        // the one that moves it out asks about a `mod` that only just appeared.
        //
        // The readiness wait polls `textDocument/hover`, so it is pointed at the module's *name*
        // rather than the `mod` keyword the assist itself anchors on: a name is a thing the server
        // always has something to say about, and a keyword is not. Waiting on the keyword worked, but
        // a null hover there would spend the whole resolution budget before failing, and would report
        // the timeout as an indexing problem rather than as what it was.
        let named = Position {
            line: caret.start.line,
            col: caret.start.col + MOD_KEYWORD.len() as u32,
        };
        let start = lsp_range(Range {
            start: named,
            end: named,
        })["start"]
            .clone();
        self.wait_until_resolved(produced.uri, &start)?;

        let action = self.assist(produced.uri, caret, RefactorKind::ExtractModuleToFile)?;
        let resolved = self.request_settled("codeAction/resolve", action)?;
        let workspace_edit = resolved.get("edit").unwrap_or(&resolved);

        let mut changes = Vec::new();
        let mut created: Vec<String> = Vec::new();
        let mut parent = produced.text.to_string();

        for change in document_changes(workspace_edit) {
            // The parent alone needs special handling: its base is the grouped text this operation
            // produced along the way, which never reaches disk. Every other file the assist touches is
            // based on the tree, which is what `convert_change` already assumes.
            if edits_the_parent(&change, produced.relative, workspace)? {
                parent = apply_lsp_edit(&parent, edits_in(&change)?);
                continue;
            }
            changes.extend(convert_change(&change, workspace, &mut created)?);
        }

        changes.push(FileEdit::Change {
            path: produced.relative.to_string(),
            edits: minimal_edits(produced.original, &parent),
        });

        Ok(changes)
    }

    /// The files one assist's result touches: the anchor's own, or the parent plus the module it
    /// spawned when `to_file` asked for both steps at once.
    fn edit_for(
        &mut self,
        op: &RefactorOp,
        produced: Produced<'_>,
        workspace: &Workspace<'_>,
    ) -> Result<WorkspaceEdit> {
        if op.op == RefactorKind::ExtractModule && op.to_file {
            let module = op
                .name
                .as_deref()
                .ok_or_else(|| failure("the operation needs a name"))?;
            return Ok(WorkspaceEdit {
                changes: self.chain_module_to_file(produced, module, workspace)?,
            });
        }

        Ok(WorkspaceEdit {
            changes: vec![FileEdit::Change {
                path: produced.relative.to_string(),
                edits: minimal_edits(produced.original, produced.text),
            }],
        })
    }

    fn multi_file_assist(
        &mut self,
        uri: &str,
        workspace: &Workspace<'_>,
        op: &RefactorOp,
    ) -> Result<WorkspaceEdit> {
        let range = self.anchor_range(uri, op)?;

        let action = self.assist(uri, range, op.op)?;
        let resolved = self.request_settled("codeAction/resolve", action)?;
        let workspace_edit = resolved.get("edit").unwrap_or(&resolved);

        let mut changes = Vec::new();
        let mut created: Vec<String> = Vec::new();

        for change in document_changes(workspace_edit) {
            changes.extend(convert_change(&change, workspace, &mut created)?);
        }

        if changes.is_empty() {
            return Err(failure(format!(
                "rust-analyzer returned no edits for {:?}",
                op.op
            )));
        }
        Ok(WorkspaceEdit { changes })
    }

    /// The range an operation's anchor names.
    ///
    /// A symbol anchor answers with the position of the declaration's own name, which is where
    /// rust-analyzer offers the assists that act on a whole item. Resolving one waits for the crate
    /// graph, because an assist that reads a symbol's callers is not offered until they resolve.
    fn anchor_range(&mut self, uri: &str, op: &RefactorOp) -> Result<Range> {
        match &op.anchor {
            Anchor::Range { start, end, .. } => Ok(Range {
                start: *start,
                end: *end,
            }),
            Anchor::Symbol { path, .. } => {
                let position = self.locate_symbol(uri, path)?;
                self.wait_until_resolved(uri, &position)?;
                let point = LspPoint::read(Some(&position))?;
                let start = Position {
                    line: point.line as u32 + 1,
                    col: point.character as u32 + 1,
                };
                Ok(Range { start, end: start })
            }
        }
    }

    /// Rename the symbol an anchor points at, using the server's own rename.
    ///
    /// Both anchor kinds work: a range names a position directly, and a symbol is resolved through
    /// `workspace/symbol` so a plan need not carry different anchors per language.
    fn rename_symbol(&mut self, uri: &str, original: &str, op: &RefactorOp) -> Result<String> {
        let name = op
            .name
            .clone()
            .ok_or_else(|| failure("rename_symbol needs a name"))?;
        let position = match &op.anchor {
            Anchor::Range { start, .. } => {
                json!({ "line": start.line - 1, "character": start.col - 1 })
            }
            Anchor::Symbol { path, .. } => self.locate_symbol(uri, path)?,
        };

        self.wait_until_resolved(uri, &position)?;

        let renamed = self.request_settled(
            "textDocument/rename",
            json!({ "textDocument": { "uri": uri }, "position": position, "newName": name }),
        )?;
        Ok(apply_lsp_edit(original, edits_for(&renamed, uri)?))
    }

    /// Wait, once per process, for the crate graph to load — with the server's progress on screen.
    ///
    /// Every request that needs name resolution is answered emptily until rust-analyzer has loaded
    /// the graph, so each loop that waits on one used to carry the whole indexing budget of its own.
    /// On a real crate that is paid per operation, and `survey_moved_items` pays it per moved item.
    /// Paying it once here is what lets every later wait be short.
    ///
    /// Hover is the authority, because it is the cheapest request that needs the graph and it is the
    /// same signal a rename is gated on. `serverStatus` is only a shortcut out: it is an extension,
    /// so a server that never sends it still has to get past this.
    ///
    /// A document with no symbols has nothing to hover, so the warm-up is skipped rather than spent
    /// on a position that would never resolve — which leaves `indexed` false, and the first real
    /// wait holding the full budget it would have had.
    fn ensure_indexed(&mut self, uri: &str) -> Result<()> {
        if self.indexed {
            return Ok(());
        }
        let symbols = self.request_settled(
            "textDocument/documentSymbol",
            json!({ "textDocument": { "uri": uri } }),
        )?;
        let Some(probe) = first_symbol_position(&symbols) else {
            return Ok(());
        };

        let started = Instant::now();
        let deadline = started + self.warmup;
        loop {
            let hover = self.request_settled(
                "textDocument/hover",
                json!({ "textDocument": { "uri": uri }, "position": probe }),
            )?;

            if !hover.is_null() || self.chatter.quiescent {
                self.indexed = true;
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(RestructureError::IndexingIncomplete {
                    environment: self.environment.clone(),
                    seconds: started.elapsed().as_secs(),
                    last: self
                        .chatter
                        .last
                        .clone()
                        .unwrap_or_else(|| "nothing reported".to_string()),
                });
            }
            std::thread::sleep(INDEXING_POLL);
        }
    }

    /// How long the next wait for name resolution may run.
    ///
    /// Before the warm-up has succeeded this is the whole indexing budget, because whatever the
    /// caller is waiting on, what it is really waiting on is the graph. After it, the graph is
    /// loaded and the only thing left to wait out is the server catching up with this client's own
    /// edits.
    fn resolution_budget(&self) -> Duration {
        if self.indexed {
            SETTLE_BUDGET
        } else {
            self.warmup
        }
    }

    /// Block until the server can resolve names at `position`.
    ///
    /// `documentSymbol` is answered from the syntax tree and so succeeds immediately, but a rename
    /// needs the crate graph. Hover is the cheapest request that also needs it, so a non-null hover
    /// is the signal that a rename will be accepted.
    fn wait_until_resolved(&mut self, uri: &str, position: &Value) -> Result<()> {
        let started = Instant::now();
        let deadline = started + self.resolution_budget();
        loop {
            let hover = self.request_settled(
                "textDocument/hover",
                json!({ "textDocument": { "uri": uri }, "position": position }),
            )?;

            if !hover.is_null() {
                self.indexed = true;
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(RestructureError::IndexingIncomplete {
                    environment: self.environment.clone(),
                    seconds: started.elapsed().as_secs(),
                    last: self
                        .chatter
                        .last
                        .clone()
                        .unwrap_or_else(|| "nothing reported".to_string()),
                });
            }
            std::thread::sleep(INDEXING_POLL);
        }
    }

    /// Where a named symbol is declared in the open document.
    ///
    /// Asked of the document rather than the workspace, so the answer does not depend on how a URI
    /// is spelled. Polled for the same reason the assists are: until the crate graph is loaded the
    /// server answers with no symbols, and a rename against an unresolved position is refused.
    fn locate_symbol(&mut self, uri: &str, name: &str) -> Result<Value> {
        let deadline = Instant::now() + self.resolution_budget();
        loop {
            let symbols = self.request_settled(
                "textDocument/documentSymbol",
                json!({ "textDocument": { "uri": uri } }),
            )?;

            if let Some(position) = find_symbol(&symbols, name) {
                return Ok(position);
            }
            if Instant::now() >= deadline {
                return Err(failure(format!("`{name}` is not declared in this file")));
            }
            std::thread::sleep(INDEXING_POLL);
        }
    }

    /// Run the requested assist and return the document as rust-analyzer left it.
    fn extract(
        &mut self,
        uri: &str,
        original: &str,
        range: Range,
        kind: RefactorKind,
    ) -> Result<String> {
        let action = self.assist(uri, range, kind)?;
        let resolved = self.request_settled("codeAction/resolve", action)?;
        Ok(apply_lsp_edit(original, edits_for(&resolved, uri)?))
    }

    /// Give the extracted function its real name.
    ///
    /// The rename is asked of the server rather than performed here, so no identifier in the
    /// result — and no reference to it anywhere else — is written by this backend.
    fn rename_placeholder(
        &mut self,
        uri: &str,
        extracted: &str,
        placeholder: Placeholder,
        name: &str,
    ) -> Result<String> {
        self.did_change(uri, extracted)?;

        let declaration = placeholder.declaration();
        let definition = extracted
            .find(&declaration)
            .map(|offset| offset + placeholder.identifier_offset())
            .ok_or_else(|| {
                failure(format!(
                    "rust-analyzer did not produce a `{declaration}` to name"
                ))
            })?;

        // An assist that introduces a top-level item leaves the server rebuilding the module tree,
        // and a rename that arrives first is refused outright rather than deferred.
        let position = position_at(extracted, definition);
        self.wait_until_resolved(uri, &position)?;

        let renamed = self.request_settled(
            "textDocument/rename",
            json!({
                "textDocument": { "uri": uri },
                "position": position,
                "newName": name
            }),
        )?;
        Ok(apply_lsp_edit(extracted, edits_for(&renamed, uri)?))
    }
}

/// What the seam survey saw, as one line.
///
/// The survey returning an empty list while the cut was detected correctly is what two rounds of
/// reasoning got wrong and one run of this settled, so the line names both halves.
fn seam_trace(range: Range, cut: &[String], relocated: &[PathReached]) -> String {
    format!(
        "seam {}:{}..{}:{} cuts {:?}; relocates {:?}",
        range.start.line,
        range.start.col,
        range.end.line,
        range.end.col,
        cut,
        relocated
            .iter()
            .map(|item| (item.name.as_str(), item.within.as_slice()))
            .collect::<Vec<_>>()
    )
}

/// The one file an in-place assist rewrote, and what it rewrote it from.
///
/// Four `&str` arguments in a row is where one quietly takes another's place, and two of these — the
/// file as it stands on disk and the text the assist produced — differ in exactly the way a swap would
/// not show up until the edit was applied.
struct Produced<'a> {
    uri: &'a str,
    /// The file's path, relative to the workspace root.
    relative: &'a str,
    /// The file as it stands on disk: the base every edit this operation reports is measured from.
    original: &'a str,
    /// What the assist produced, which for a chained operation never reaches disk.
    text: &'a str,
}

/// Whether a document change is a text edit addressing the parent, rather than another file.
fn edits_the_parent(change: &Value, relative: &str, workspace: &Workspace<'_>) -> Result<bool> {
    if change.get("kind").is_some() {
        return Ok(false);
    }
    let path = relative_path(change.pointer("/textDocument/uri"), workspace.root)?;
    Ok(path == relative)
}

/// One module-level item, as the server reports its extent.
///
/// A named struct rather than a tuple because four numbers with the same type are exactly where an
/// argument swaps places with its neighbour unnoticed.
struct OutlineItem {
    name: String,
    start_line: u32,
    end_line: u32,
    end_column: u32,
}

impl OutlineItem {
    fn read(symbol: &Value) -> Option<OutlineItem> {
        Some(OutlineItem {
            name: symbol.get("name")?.as_str()?.to_string(),
            start_line: symbol.pointer("/range/start/line")?.as_u64()? as u32,
            end_line: symbol.pointer("/range/end/line")?.as_u64()? as u32,
            end_column: symbol.pointer("/range/end/character")?.as_u64()? as u32,
        })
    }
}

/// Where each named item sits in the outline, sorted, refusing a name the file does not define.
///
/// An unknown name is a mistake in the request rather than an empty range, so it is named back.
fn places_of(outline: &[OutlineItem], items: &[String], file: &str) -> Result<Vec<usize>> {
    let mut places = Vec::new();

    for item in items {
        let at = outline
            .iter()
            .position(|candidate| &candidate.name == item)
            .ok_or_else(|| {
                failure(format!(
                    "`{item}` is not an item `{file}` defines at module level"
                ))
            })?;
        places.push(at);
    }

    places.sort_unstable();
    Ok(places)
}

/// Refuse a run of items with anything between them.
///
/// A seam is one contiguous range, so a span reaching from one item to a distant one would carry
/// everything in between — silently, and with nothing in the plan to show it.
fn refuse_non_adjacent(outline: &[OutlineItem], places: &[usize]) -> Result<()> {
    let Some(gap) = places.windows(2).find(|pair| pair[1] != pair[0] + 1) else {
        return Ok(());
    };

    Err(failure(format!(
        "the named items are not adjacent: `{}` and `{}` have `{}` between them, and a seam is one \
         contiguous range — a span reaching from one to the other would carry everything in between.",
        outline[gap[0]].name,
        outline[gap[1]].name,
        outline[gap[0] + 1].name
    )))
}

/// The first line of the comment block and attributes attached to the item on `line`.
///
/// rust-analyzer attaches a preceding comment block to the item below it *unless a blank line
/// separates them*, so a range that starts at the `pub fn` leaves the doc comment behind in the
/// parent and the relocated item arrives undocumented. Walking up and stopping at the blank is the
/// whole rule.
///
/// Idempotent where the server already reports an extent that includes the trivia: the walk stops at
/// the blank line immediately and returns what it was given.
fn attached_trivia_starts_at(text: &str, line: u32) -> u32 {
    let lines: Vec<&str> = text.split('\n').collect();
    let mut first = line;

    while first > 1 {
        let above = lines
            .get(first as usize - 2)
            .map(|text| text.trim())
            .unwrap_or_default();
        if above.starts_with("///") || above.starts_with("//") || above.starts_with("#[") {
            first -= 1;
            continue;
        }
        break;
    }

    first
}

/// The keyword a module declaration opens with, including its trailing space.
const MOD_KEYWORD: &str = "mod ";

/// A caret on the `mod` keyword of a module the extraction just wrote.
///
/// The declaration is the one piece of text an extraction invents, so its position cannot come from
/// the plan — no snapshot coordinate maps to it. It is found in the produced text instead, which is
/// exactly why the two steps needed two plans until one operation did both.
fn caret_at_module(text: &str, module: &str) -> Result<Range> {
    let needle = format!("{MOD_KEYWORD}{module}");

    for (index, line) in text.split('\n').enumerate() {
        let Some(at) = whole_word(line, &needle) else {
            continue;
        };
        let point = Position {
            line: index as u32 + 1,
            col: at as u32 + 1,
        };
        return Ok(Range {
            start: point,
            end: point,
        });
    }

    Err(failure(format!(
        "the extraction left no `{needle}` for `to_file` to move out"
    )))
}

/// Where `needle` occurs in `line` as a whole declaration rather than the prefix of a longer one.
///
/// A substring search finds `mod ranking` when asked for `mod rank`, and the assist would then move
/// the wrong declaration — silently, because both are real modules and both extract cleanly.
fn whole_word(line: &str, needle: &str) -> Option<usize> {
    let bytes = line.as_bytes();
    let mut from = 0;

    while let Some(found) = line[from..].find(needle) {
        let start = from + found;
        let end = start + needle.len();
        let opens = start == 0 || !is_identifier_byte(bytes[start - 1]);
        let closes = end >= bytes.len() || !is_identifier_byte(bytes[end]);
        if opens && closes {
            return Some(start);
        }
        from = start + 1;
    }

    None
}

/// One replacement the language server asked for, in the server's own coordinates.
struct LspEdit {
    start: LspPoint,
    end: LspPoint,
    new_text: String,
}

/// A zero-based line/character pair. Ordered so edits can be sorted before being resolved to
/// byte offsets.
#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
struct LspPoint {
    line: usize,
    character: usize,
}

impl LspPoint {
    /// Read a position, refusing a malformed one rather than defaulting to the top of the file —
    /// a silently wrong position would corrupt the source it is applied to.
    fn read(value: Option<&Value>) -> Result<LspPoint> {
        let value = value.ok_or_else(|| failure("edit is missing a position"))?;
        let field = |name: &str| {
            value
                .get(name)
                .and_then(Value::as_u64)
                .ok_or_else(|| failure(format!("position is missing `{name}`")))
        };
        Ok(LspPoint {
            line: field("line")? as usize,
            character: field("character")? as usize,
        })
    }
}

/// The start of a named symbol's selection range, searching nested symbols depth-first.
fn find_symbol(symbols: &Value, name: &str) -> Option<Value> {
    for symbol in symbols.as_array()? {
        if symbol.get("name").and_then(Value::as_str) == Some(name) {
            return symbol
                .pointer("/selectionRange/start")
                .or_else(|| symbol.pointer("/location/range/start"))
                .cloned();
        }
        if let Some(found) = symbol
            .get("children")
            .and_then(|kids| find_symbol(kids, name))
        {
            return Some(found);
        }
    }
    None
}

/// Where the document's first declared symbol is named — the cheapest position in a file that is
/// guaranteed to resolve once the crate graph is loaded, and so the one the warm-up probes.
fn first_symbol_position(symbols: &Value) -> Option<Value> {
    symbols
        .as_array()?
        .first()?
        .pointer("/selectionRange/start")
        .or_else(|| {
            symbols
                .as_array()?
                .first()?
                .pointer("/location/range/start")
        })
        .cloned()
}

/// The filesystem path a `file://` uri names, the inverse of `uri_of`.
fn path_of(uri: &str) -> Result<PathBuf> {
    uri.strip_prefix("file://")
        .map(PathBuf::from)
        .ok_or_else(|| failure(format!("`{uri}` is not a file uri")))
}

/// The symbols a range covers that a caller could name by *module path*, each with the position to
/// ask the server about it at.
///
/// Two kinds of nesting, and the difference is the whole point. A symbol inside a **module** is
/// reached by a path that grows with it — `outer::inner::item` — so relocating the module changes
/// that path and the symbol has to be checked. Not descending is why an item one level down was
/// never inspected at all, and an extraction that should have refused reported success.
///
/// A symbol inside anything else — an `impl`, a struct, an enum — is reached through its *type*:
/// `manager.execute_mutation(…)` resolves through `Manager`, not through whichever module holds the
/// `impl`. Those travel with the item that carries them and no caller anywhere changes, so they are
/// not path-reached and are deliberately not checked. Splitting an oversized `impl` is free.
fn path_reached_within(symbols: &Value, range: Range) -> Vec<PathReached> {
    let mut found = Vec::new();
    collect_path_reached(symbols, range, &[], &mut found);
    found
}

/// One item a relocation would move, and where inside the range it sits.
struct PathReached {
    name: String,
    /// The inline modules inside the range that hold it, outermost first. Empty for an item the
    /// range holds directly — which is every item a facade can name flat under the new module.
    within: Vec<String>,
    /// Where the server reported the item's name, which is the position a reference query asks about.
    position: Value,
}

fn collect_path_reached(
    symbols: &Value,
    range: Range,
    within: &[String],
    found: &mut Vec<PathReached>,
) {
    for symbol in symbols.as_array().into_iter().flatten() {
        if !covers(range, symbol.pointer("/range/start")) {
            continue;
        }

        let name = symbol.get("name").and_then(Value::as_str);

        // An `impl` block is reported under the name `impl Gauge`, which no module path can spell.
        // Collecting it produced `use gauging::{impl Gauge};` — a syntax error on top of the `E0252`
        // the moved type had already earned — so a name that is not a single identifier is dropped
        // here rather than carried to a facade that cannot write it.
        if let (Some(name), Some(position)) = (
            name.filter(|name| is_identifier(name)),
            symbol
                .pointer("/selectionRange/start")
                .or_else(|| symbol.pointer("/location/range/start")),
        ) {
            found.push(PathReached {
                name: name.to_string(),
                within: within.to_vec(),
                position: position.clone(),
            });
        }

        // Descend only through modules. Everything below anything else is reached through a type.
        if symbol.get("kind").and_then(Value::as_u64) == Some(SYMBOL_KIND_MODULE) {
            if let Some(children) = symbol.get("children") {
                let mut inside = within.to_vec();
                inside.extend(name.map(str::to_owned));
                collect_path_reached(children, range, &inside, found);
            }
        }
    }
}

/// The visibility an item was written with, read from the text preceding its name.
///
/// `pub fn render` gives `pub`, `pub(crate) fn tier` gives `pub(crate)`, and `fn normalise` gives the
/// empty string — module-private, which is the one the assist does not preserve.
fn visibility_at(text: &str, position: &Value) -> String {
    let Ok(point) = LspPoint::read(Some(position)) else {
        return String::new();
    };
    let Some(line) = text.split('\n').nth(point.line) else {
        return String::new();
    };
    let prefix = match line.char_indices().nth(point.character) {
        Some((index, _)) => &line[..index],
        None => line,
    };
    visibility_in(prefix)
}

/// The `pub…` prefix of a declaration, if it has one.
fn visibility_in(prefix: &str) -> String {
    let trimmed = prefix.trim_start();
    let Some(rest) = trimmed.strip_prefix("pub") else {
        return String::new();
    };

    if !rest.starts_with('(') {
        // Guard against an identifier that merely begins with `pub`, such as `publish`.
        let separated = rest.is_empty() || rest.starts_with(|c: char| !is_identifier_char(c));
        return if separated {
            "pub".to_string()
        } else {
            String::new()
        };
    }

    match rest.find(')') {
        Some(close) => format!("pub{}", &rest[..=close]),
        None => "pub".to_string(),
    }
}

fn is_identifier_char(character: char) -> bool {
    character.is_ascii_alphanumeric() || character == '_'
}

/// Whether a name is a single Rust identifier, and so a name a `use` declaration can write.
fn is_identifier(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with(|c: char| c.is_ascii_digit())
        && name.chars().all(is_identifier_char)
}

/// Whether a one-based range contains a zero-based LSP position.
fn covers(range: Range, position: Option<&Value>) -> bool {
    let Ok(point) = LspPoint::read(position) else {
        return false;
    };
    let line = point.line as u32 + 1;

    line >= range.start.line && line <= range.end.line
}

/// The `context` of a code action request; an empty kind list means unfiltered.
fn context_for(kinds: &[&str]) -> Value {
    if kinds.is_empty() {
        json!({ "diagnostics": [] })
    } else {
        json!({ "diagnostics": [], "only": kinds })
    }
}

/// Every entry of a workspace edit's `documentChanges`, in the order the server gave them.
fn document_changes(workspace_edit: &Value) -> Vec<Value> {
    workspace_edit
        .get("documentChanges")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

/// The workspace-relative path a `file://` URI names.
fn relative_path(uri: Option<&Value>, root: &Path) -> Result<String> {
    let uri = uri
        .and_then(Value::as_str)
        .ok_or_else(|| failure("a document change carries no uri"))?;
    let path = uri
        .strip_prefix("file://")
        .ok_or_else(|| failure(format!("`{uri}` is not a file uri")))?;

    Path::new(path)
        .strip_prefix(root)
        .map(|relative| relative.display().to_string())
        .map_err(|_| failure(format!("`{path}` lies outside the workspace")))
}

/// The text edits one `documentChanges` entry carries.
fn edits_in(change: &Value) -> Result<Vec<LspEdit>> {
    change
        .get("edits")
        .and_then(Value::as_array)
        .ok_or_else(|| failure("a document change carries no edits"))?
        .iter()
        .map(read_edit)
        .collect()
}

/// The one import to apply out of everything the server offered for a name.
///
/// A single offer is the answer. Several mean the name is worn by several items, and the one the
/// moved code meant is the one the file was already importing before the move carried it out of
/// that declaration's scope. Reading those declarations back narrows the choice without inventing a
/// path — rust-analyzer still writes every character of the `use` it inserts. A name the file's own
/// imports do not settle has no answer here either, and is reported rather than picked.
fn choose_import<'a>(text: &str, offered: &[&'a str]) -> Option<&'a str> {
    if let [only] = offered {
        return Some(only);
    }

    let in_scope = imported_paths(text);
    let mut narrowed = offered
        .iter()
        .filter(|title| import_path(title).is_some_and(|path| in_scope.contains(&path)));

    let only = *narrowed.next()?;
    narrowed.next().is_none().then_some(only)
}

/// `source` without the single-name `use` lines inside `block` that no import can be.
///
/// A line goes when the server reports an unresolved name on it — the import binds nothing — or when
/// the name it binds is bound again by another `use` in the same block, which the compiler rejects
/// whichever of the two resolves. A grouped `use` is never dropped: it carries names beyond the one
/// in question.
fn without_dead_imports(
    source: &[String],
    block: &ModuleBlock,
    unresolved: &[UnresolvedName],
) -> Vec<String> {
    let inside = |index: usize| index > block.opened && index < block.closed;

    // Names bound by a group have to be known before the simple lines are judged: the duplicate is
    // as often written above its group as below it.
    let grouped: Vec<String> = source
        .iter()
        .enumerate()
        .filter(|(index, line)| inside(*index) && simple_import(line).is_none())
        .flat_map(|(_, line)| bound_names(line))
        .collect();

    let mut kept = Vec::with_capacity(source.len());
    let mut standing: Vec<String> = Vec::new();

    for (index, line) in source.iter().enumerate() {
        if let Some(name) = simple_import(line).filter(|_| inside(index)) {
            if unresolved_on_line(unresolved, index)
                || grouped.contains(&name)
                || standing.contains(&name)
            {
                continue;
            }
            standing.push(name);
        }
        kept.push(line.clone());
    }

    kept
}

/// The one name a `use` line binds, when it is a single-line declaration binding exactly one.
fn simple_import(line: &str) -> Option<String> {
    if line.contains('{') || !line.trim_end().ends_with(';') {
        return None;
    }

    let names = bound_names(line);
    let [only] = names.as_slice() else {
        return None;
    };
    Some(only.clone())
}

/// The final segment of every path a line's `use` declaration binds.
fn bound_names(line: &str) -> Vec<String> {
    imported_paths(line)
        .iter()
        .filter_map(|path| path.rsplit("::").next().map(str::to_string))
        .collect()
}

/// Whether the server reports any unresolved name on the given zero-based line.
fn unresolved_on_line(unresolved: &[UnresolvedName], index: usize) -> bool {
    unresolved
        .iter()
        .any(|found| found.position.get("line").and_then(Value::as_u64) == Some(index as u64))
}

/// How many of the names the server could not resolve are `name`.
fn occurrences_of(unresolved: &[UnresolvedName], name: &str) -> usize {
    unresolved.iter().filter(|found| found.text == name).count()
}

/// The order to try the offered imports in: the one this file's own imports point at first, then
/// every other path the server offered, as it offered them.
///
/// `choose_import` still decides which path the moved code *meant*, and still refuses when neither
/// the server nor the file settles it. What is new is that its answer is a first guess rather than
/// the only one — rust-analyzer offers paths that resolve nothing, and the caller verifies each in
/// turn instead of trusting the title.
fn import_order<'a>(text: &str, offered: &'a [String]) -> Option<Vec<&'a str>> {
    let borrowed: Vec<&str> = offered.iter().map(String::as_str).collect();
    let first = choose_import(text, &borrowed)?;

    let mut ordered = vec![first];
    ordered.extend(borrowed.iter().copied().filter(|title| *title != first));
    Some(ordered)
}

/// Whether the identifier at `position` is reached through a qualifier — `Type::assoc`, `value.field`.
///
/// Such a name is an associated item or a field, resolved through what precedes it rather than
/// through a path, so no `use` can bind it. A range's `..` is excluded: the name after it is an
/// ordinary expression, and a constant there is importable like any other.
fn reached_through_qualifier(text: &str, position: &Value) -> bool {
    let (Some(line), Some(character)) = (
        position.get("line").and_then(Value::as_u64),
        position.get("character").and_then(Value::as_u64),
    ) else {
        return false;
    };

    let Some(before) = text
        .split('\n')
        .nth(line as usize)
        .and_then(|source| source.get(..character as usize))
        .map(str::trim_end)
    else {
        return false;
    };

    before.ends_with("::") || (before.ends_with('.') && !before.ends_with(".."))
}

/// Whether `module`'s own `use` declarations already bind `name`.
///
/// Scoped to the block being repaired rather than read over the whole file: the same name is
/// routinely imported by a sibling module, and treating that as already bound here would skip an
/// import the moved code genuinely lost.
fn already_bound(text: &str, module: &str, name: &str) -> Result<bool> {
    let source: Vec<String> = text.split('\n').map(str::to_string).collect();
    let block = module_bounds(&source, module)?;
    let body = source[block.opened..block.closed].join("\n");

    Ok(imported_paths(&body)
        .iter()
        .any(|path| path.rsplit("::").next() == Some(name)))
}

/// The path an `Import` quickfix names, read out of its title.
fn import_path(title: &str) -> Option<String> {
    let named = title.strip_prefix(IMPORT_TITLE)?;
    Some(named.trim().trim_matches('`').to_string())
}

/// Every path the text's own `use` declarations bind, with their `{…}` groups expanded.
///
/// This is a lexical read, not a resolution: a glob contributes nothing because there is no telling
/// what it brings in, and a `use` written inside a function counts the same as one at the top. Both
/// only ever cost a refusal, never a wrong import.
fn imported_paths(text: &str) -> Vec<String> {
    let mut paths = Vec::new();

    for statement in text.split(';') {
        if let Some(tree) = use_tree(statement) {
            expand_use(tree, "", &mut paths);
        }
    }

    paths
}

/// The `use` tree a semicolon-terminated statement declares, if it declares one.
///
/// The statement is read from its last line-initial `use`, which keeps a `{…}` group spanning
/// several lines whole while leaving whatever precedes the declaration — an earlier statement, a
/// comment quoting an import — out of it.
fn use_tree(statement: &str) -> Option<&str> {
    let mut start = None;
    let mut offset = 0;

    for line in statement.split_inclusive('\n') {
        let trimmed = line.trim_start();
        if trimmed.starts_with("use ") || trimmed.starts_with("pub use ") {
            start = Some(offset + line.len() - trimmed.len());
        }
        offset += line.len();
    }

    let tree = &statement[start?..];
    tree.strip_prefix("use ")
        .or_else(|| tree.strip_prefix("pub use "))
}

/// Push every path a `use` tree binds, expanding each group onto the prefix that leads to it.
fn expand_use(tree: &str, prefix: &str, paths: &mut Vec<String>) {
    let tree = tree.trim();

    let Some(open) = tree.find('{') else {
        let bound = tree.split(" as ").next().unwrap_or(tree).trim();
        if bound == "self" {
            paths.push(prefix.trim_end_matches("::").to_string());
        } else if !bound.is_empty() && !bound.ends_with('*') {
            paths.push(format!("{prefix}{bound}"));
        }
        return;
    };

    let head = format!("{prefix}{}", &tree[..open]);
    let close = tree.rfind('}').unwrap_or(tree.len());

    for member in group_members(&tree[open + 1..close]) {
        expand_use(member, &head, paths);
    }
}

/// A group's members, split on the commas that are not inside a group of their own.
fn group_members(inner: &str) -> Vec<&str> {
    let mut members = Vec::new();
    let mut depth = 0usize;
    let mut start = 0;

    for (offset, character) in inner.char_indices() {
        match character {
            '{' => depth += 1,
            '}' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                members.push(&inner[start..offset]);
                start = offset + 1;
            }
            _ => {}
        }
    }
    members.push(&inner[start..]);

    members
}

/// An identifier the server could not resolve, with the position an assist is asked for at.
struct UnresolvedName {
    text: String,
    position: Value,
}

/// Where `wanted` sits in the semantic-token legend the server answered the handshake with.
fn token_type_index(handshake: &Value, wanted: &str) -> Option<u32> {
    handshake
        .pointer("/capabilities/semanticTokensProvider/legend/tokenTypes")?
        .as_array()?
        .iter()
        .position(|entry| entry.as_str() == Some(wanted))
        .map(|index| index as u32)
}

/// Decode a semantic-token response, keeping the tokens of one type.
///
/// The protocol encodes tokens as a flat run of five integers each — line delta, start delta,
/// length, type, modifiers — where the deltas are relative to the token before, and the start
/// delta restarts at the beginning of every new line.
fn unresolved_in(tokens: &Value, wanted: u32, text: &str) -> Vec<UnresolvedName> {
    let lines: Vec<&str> = text.split('\n').collect();
    let data: Vec<u32> = tokens
        .get("data")
        .and_then(Value::as_array)
        .map(|entries| {
            entries
                .iter()
                .filter_map(Value::as_u64)
                .map(|n| n as u32)
                .collect()
        })
        .unwrap_or_default();

    let mut found = Vec::new();
    let mut line: u32 = 0;
    let mut character: u32 = 0;

    for entry in data.chunks_exact(5) {
        let [line_delta, start_delta, length, token_type, _] = *entry else {
            continue;
        };
        line += line_delta;
        character = if line_delta == 0 {
            character + start_delta
        } else {
            start_delta
        };

        if token_type != wanted {
            continue;
        }

        // A token whose span does not land inside the document is not one to act on: the server is
        // describing a version of the file this client no longer holds.
        let Some(name) = lines
            .get(line as usize)
            .and_then(|source| source.get(character as usize..(character + length) as usize))
        else {
            continue;
        };

        found.push(UnresolvedName {
            text: name.to_string(),
            position: json!({ "line": line, "character": character }),
        });
    }

    found
}

/// The text edits an LSP workspace edit carries for one document.
///
/// Servers may answer in either shape the protocol allows — `documentChanges` or `changes` — and a
/// bare `WorkspaceEdit` (as `textDocument/rename` returns) or one wrapped in a code action.
fn edits_for(response: &Value, uri: &str) -> Result<Vec<LspEdit>> {
    let workspace_edit = response.get("edit").unwrap_or(response);

    let raw: Vec<Value> = match workspace_edit
        .get("documentChanges")
        .and_then(Value::as_array)
    {
        Some(changes) => changes
            .iter()
            .filter(|change| {
                change.pointer("/textDocument/uri").and_then(Value::as_str) == Some(uri)
            })
            .filter_map(|change| change.get("edits").and_then(Value::as_array))
            .flatten()
            .cloned()
            .collect(),
        None => workspace_edit
            .pointer(&format!("/changes/{}", uri.replace('/', "~1")))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
    };

    if raw.is_empty() {
        return Err(failure("rust-analyzer returned no edits for the document"));
    }

    raw.iter().map(read_edit).collect()
}

/// One text edit, refusing anything malformed rather than guessing at it.
fn read_edit(entry: &Value) -> Result<LspEdit> {
    Ok(LspEdit {
        start: LspPoint::read(entry.pointer("/range/start"))?,
        end: LspPoint::read(entry.pointer("/range/end"))?,
        new_text: entry
            .get("newText")
            .and_then(Value::as_str)
            .ok_or_else(|| failure("edit is missing `newText`"))?
            .to_string(),
    })
}

/// Apply LSP edits to `text`. Ranges share one coordinate space, so they land last-first.
fn apply_lsp_edit(text: &str, mut edits: Vec<LspEdit>) -> String {
    edits.sort_by_key(|edit| edit.start);

    let mut result = text.to_string();
    for edit in edits.into_iter().rev() {
        let from = offset_of(&result, edit.start);
        let to = offset_of(&result, edit.end);
        result.replace_range(from..to.max(from), &edit.new_text);
    }
    result
}

/// Zero-based line/character to a byte offset.
fn offset_of(text: &str, point: LspPoint) -> usize {
    let mut offset = 0usize;
    for _ in 0..point.line {
        match text[offset..].find('\n') {
            None => return text.len(),
            Some(index) => offset += index + 1,
        }
    }
    let end = text[offset..].find('\n').map_or(text.len(), |i| offset + i);

    // `character` is a byte offset into the line, per the encoding declared at initialize, so it is
    // added rather than walked. The boundary walk is for a server that broke that agreement: landing
    // mid-character would panic on the next slice, and advancing to the next boundary is the one
    // recovery that cannot.
    let mut at = offset + point.character.min(end - offset);
    while at < end && !text.is_char_boundary(at) {
        at += 1;
    }
    at
}

/// Byte offset to the zero-based LSP position naming it.
fn position_at(text: &str, offset: usize) -> Value {
    let before = &text[..offset];
    let line = before.matches('\n').count();
    let character = before.rsplit('\n').next().map_or(0, str::len);
    json!({ "line": line, "character": character })
}

fn lsp_range(range: Range) -> Value {
    json!({
        "start": { "line": range.start.line - 1, "character": range.start.col - 1 },
        "end": { "line": range.end.line - 1, "character": range.end.col - 1 }
    })
}

/// Refuse an extraction whose signature carries an inferred-type placeholder.
///
/// rust-analyzer writes the types in an extracted item's signature from its own inference. Asked
/// before inference is ready it can still produce the assist — the *shape* comes from the syntax tree
/// — and fills what it does not yet know with `_`:
///
/// ```ignore
/// fn compute_spread(sample: &Sample) -> (_, _) {
/// ```
///
/// `_` is not legal in an item signature (`E0121`), so the result compiles nowhere and the operation
/// would otherwise report success. Waiting for the anchor to resolve is what avoids this; this is the
/// post-condition that keeps a recurrence loud instead of writing a tree that cannot build.
fn refuse_inferred_placeholder(text: &str, declaration: &str) -> Result<()> {
    let Some(line) = text
        .split('\n')
        .find(|line| line.contains(declaration) && carries_placeholder_type(line))
    else {
        return Ok(());
    };

    Err(failure(format!(
        "rust-analyzer wrote `{}` — it produced the extraction before it could infer the types the \
         signature needs, and `_` is not legal there (E0121). The crate graph was most likely still \
         loading; retrying the operation against a warm server resolves it.",
        line.trim()
    )))
}

/// Whether a declaration line carries `_` where a type belongs.
///
/// Tokenised on identifier boundaries, so `fun_name` and `var_name` — which contain an underscore but
/// are not one — do not register.
fn carries_placeholder_type(line: &str) -> bool {
    line.split(|character: char| !is_identifier_char(character))
        .any(|token| token == "_")
}

/// Refuse a result the placeholder survived into.
///
/// The assist rewrites references to the items it moved as `modname::Item`, and the rename that
/// follows is asked of the server — so every reference it can resolve is renamed along with the
/// declaration. One it *cannot* resolve is left exactly as it was: from inside a different,
/// already-extracted module `modname::Item` never named anything, and rust-analyzer does not rename
/// an unresolved path. What lands is source that compiles nowhere, from an operation that reported
/// success, which is the worst outcome this tool has.
///
/// The count is compared against the text as it stood before the assist ran rather than against
/// zero, so a file that legitimately contains the identifier is not refused for containing it.
fn refuse_residual_placeholder(original: &str, produced: &str, name: &str) -> Result<()> {
    let before = placeholder_sites(original, name).len();
    let sites = placeholder_sites(produced, name);
    if sites.len() <= before {
        return Ok(());
    }

    let mut lines: Vec<String> = Vec::new();
    for site in &sites {
        let line = site.to_string();
        if !lines.contains(&line) {
            lines.push(line);
        }
    }

    // Two causes, and they want opposite advice. A leftover inside an already-extracted *module* is
    // an ordering mistake: extract the definition first and no reference to it is sitting in a scope
    // the rewritten path cannot reach. A leftover inside an `impl` is not, and reordering the plan
    // provably does not help — one real split was reordered in full and produced byte-identical
    // refusals at identical offsets. An `impl` body cannot hold a `mod`, so the sibling can be moved
    // neither first nor second; only a wider seam removes the reference.
    //
    // Where both occur the `impl` wording wins, because it is the one no ordering can satisfy.
    let inside_an_impl = sites
        .iter()
        .any(|site| enclosing_block(produced, *site) == Some(Block::Impl));

    let remedy = if inside_an_impl {
        "That call sits inside an `impl`, and the module was written outside it, so no ordering of \
         this plan makes the path resolve — an `impl` body cannot hold a `mod`. Either grow the \
         seam to carry the whole `impl`, or cut it where nothing crosses."
    } else {
        "Extract a definition before the items that reference it, so no reference to it is sitting \
         inside an already-extracted module when it moves."
    };

    Err(failure(format!(
        "rust-analyzer left `{name}` behind in {} place(s) its rename could not reach, so the \
         extraction would report success over source that resolves nowhere — line(s) {}. {remedy}",
        sites.len() - before,
        lines.join(", ")
    )))
}

/// The kind of block a line sits inside, where the difference changes what a refusal should advise.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Block {
    Impl,
    Module,
}

/// The innermost `impl` or `mod` a one-based line sits inside.
///
/// Lexical, and deliberately allowed to be approximate: a brace inside a string literal would mislead
/// it. That is affordable because this only ever chooses the *wording* of a refusal that has already
/// been decided — never whether to refuse. Anything finer would mean parsing, for no change in
/// outcome.
fn enclosing_block(text: &str, line: u32) -> Option<Block> {
    let mut stack: Vec<Option<Block>> = Vec::new();

    for (index, raw) in text.split('\n').enumerate() {
        if index as u32 + 1 == line {
            return stack.iter().rev().copied().flatten().next();
        }

        let code = raw.split("//").next().unwrap_or(raw);
        let mut declaration = 0usize;

        for (at, character) in code.char_indices() {
            match character {
                '{' => {
                    stack.push(declares(&code[declaration..at]));
                    declaration = at + character.len_utf8();
                }
                '}' => {
                    stack.pop();
                    declaration = at + character.len_utf8();
                }
                ';' => declaration = at + character.len_utf8(),
                _ => {}
            }
        }
    }

    None
}

/// Which block, if either, a declaration introduces. Whole tokens, so `implement` is not `impl`.
fn declares(head: &str) -> Option<Block> {
    head.split(|character: char| !is_identifier_char(character))
        .find_map(|token| match token {
            "impl" => Some(Block::Impl),
            "mod" => Some(Block::Module),
            _ => None,
        })
}

/// Every one-based line on which `name` occurs as a whole identifier, once per occurrence.
///
/// Whole-word, because `modname` inside `modnamed` is a different name and refusing on it would make
/// perfectly good code unrefactorable.
fn placeholder_sites(text: &str, name: &str) -> Vec<u32> {
    let mut sites = Vec::new();

    for (index, line) in text.split('\n').enumerate() {
        let bytes = line.as_bytes();
        let mut from = 0;
        while let Some(found) = line[from..].find(name) {
            let start = from + found;
            let end = start + name.len();
            let opens = start == 0 || !is_identifier_byte(bytes[start - 1]);
            let closes = end >= bytes.len() || !is_identifier_byte(bytes[end]);
            if opens && closes {
                sites.push(index as u32 + 1);
            }
            from = end;
        }
    }

    sites
}

fn is_identifier_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// The difference between two versions of a file, as one edit per changed region.
///
/// One edit spanning everything between the first and last change would be simpler, and is wrong:
/// the coordinate ledger folds these edits to translate later anchors, and a position *inside* a
/// replaced span cannot be translated at all. An extraction routinely changes two distant places at
/// once — it rewrites the items it relocated and every reference to them — so a single span swallows
/// every untouched line between, and the ledger then correctly reports every anchor there as removed.
/// A plan could therefore hold only one Rust extraction, which is what forced a fresh server, and a
/// fresh server re-indexes the crate.
///
/// The common prefix and suffix are trimmed first, which bounds the search; the rest is Myers' diff,
/// the same algorithm the TypeScript sidecar runs, so the two backends cannot disagree on where a
/// hunk begins.
fn minimal_edits(previous: &str, current: &str) -> Vec<TextEdit> {
    let before: Vec<&str> = previous.split('\n').collect();
    let after: Vec<&str> = current.split('\n').collect();

    let mut prefix = 0;
    while prefix < before.len() && prefix < after.len() && before[prefix] == after[prefix] {
        prefix += 1;
    }
    let mut suffix = 0;
    while suffix < before.len() - prefix
        && suffix < after.len() - prefix
        && before[before.len() - 1 - suffix] == after[after.len() - 1 - suffix]
    {
        suffix += 1;
    }

    changed_regions(
        &before[prefix..before.len() - suffix],
        &after[prefix..after.len() - suffix],
    )
    .into_iter()
    .map(|region| TextEdit {
        range: Range {
            start: Position {
                line: (prefix + region.from) as u32 + 1,
                col: 1,
            },
            end: Position {
                line: (prefix + region.to) as u32 + 1,
                col: 1,
            },
        },
        new_text: if region.lines.is_empty() {
            String::new()
        } else {
            format!("{}\n", region.lines.join("\n"))
        },
    })
    .collect()
}

/// Where the references to one item sit, relative to the range about to be relocated.
#[derive(Default)]
struct Reach {
    /// Files other than the anchor's own that reference it.
    stranded_in: Vec<String>,
    /// Whether anything outside the range reaches it, in this file or another.
    from_outside: bool,
    /// One-based lines in this same file, outside the range, where something references it.
    ///
    /// Kept separately from `from_outside` because that flag answers a visibility question and
    /// collapses this case together with a reference in another file. Only the seam survey knows
    /// whether these lines matter, and they matter for exactly one kind of item.
    in_file_outside_at: Vec<u32>,
}

/// What the extraction knows about one item it is about to relocate.
struct MovedItem {
    name: String,
    /// The inline modules inside the relocated range that hold it, outermost first. An item with
    /// any is reached through them, so a facade that named it flat would write a path that never
    /// resolved.
    within: Vec<String>,
    /// The visibility keyword as written — `pub`, `pub(crate)`, `pub(super)`, or empty for private.
    visibility: String,
    /// Files other than the anchor's own that reference it.
    stranded_in: Vec<String>,
    /// Whether anything outside the range being relocated reaches it, in this file or another.
    reached_from_outside: bool,
    /// One-based lines, in this same file, where a sibling *inside the same `impl`* still references
    /// it after the seam moves.
    ///
    /// Distinct from `reached_from_outside`, which is true of any reference beyond the range and is
    /// only a visibility signal. This one is the blocker: the new module is written outside the
    /// `impl`, so from inside it `modname::Item` never named anything, and rust-analyzer does not
    /// rename an unresolved path.
    referenced_in_impl_at: Vec<u32>,
}

/// Refuse an extraction that would strand a reference written in another file.
///
/// Relocating items changes the path that reaches them, and rust-analyzer rewrites no reference to
/// them. Inside this file that costs nothing — the `use` the import pass restores binds the old names
/// again — but a reference in another file has nothing to rewrite it, and only the compiler would say
/// so. One does sometimes survive regardless, where the restored `use` happens to land in a module
/// the referrer sits under; that is an accident of where the seam was cut, and not something to let a
/// restructure quietly depend on.
///
/// A facade makes the whole question moot, which is why the caller skips this when one is asked for.
fn refuse_stranded(items: &[MovedItem]) -> Result<()> {
    let stranded: Vec<String> = items
        .iter()
        .filter(|item| !item.stranded_in.is_empty())
        .map(|item| format!("`{}` from {}", item.name, item.stranded_in.join(", ")))
        .collect();

    if stranded.is_empty() {
        return Ok(());
    }

    Err(failure(format!(
        "the module would be reached by a different path than the items moved into it are now, \
         and rust-analyzer rewrites no reference it did not move: {}. Ask for `reexport` to leave the \
         old path resolving through the parent, or cut the seam where these references do not reach.",
        stranded.join("; ")
    )))
}

/// Refuse a seam that lifts an `impl` member away from a sibling in that same `impl`.
///
/// The one geometry that cannot be repaired, and it is narrower than it first looks. Three cases that
/// look alike behave differently, and only the third blocks:
///
/// - A whole `impl` moves while the parent calls its methods: nothing to rewrite, because a method is
///   reached through its type. Succeeds.
/// - A path-reached item moves while the parent names it: the assist rewrites the reference and the
///   import pass restores the binding, which is why [`refuse_stranded`] says an in-file reference
///   costs nothing. Succeeds.
/// - A member is lifted out of an `impl` while a sibling in that same `impl` calls it: the new module
///   is written *outside* the impl, so from inside it `modname::Item` never named anything and the
///   rename cannot reach the call. Refuses.
///
/// Refusing the first two would turn working restructures into refusals, which is the most expensive
/// way a check can be wrong — so only `referenced_in_impl_at` is weighed here.
///
/// The prescription differs from [`refuse_residual_placeholder`]'s on purpose. Reordering is the fix
/// when the stranded reference sits in an already-extracted *module*; it cannot help here, because an
/// `impl` body cannot hold a `mod`, so the sibling can be moved neither out of the way first nor
/// after. The seam has to grow to carry both.
///
/// Refuse a seam that cuts an `impl` in half while a member left behind still calls one that moves.
///
/// The one geometry of three that cannot be repaired, and it is narrower than it first looks:
///
/// - A whole `impl` moves while the parent calls its methods: nothing to rewrite, because a method is
///   reached through its type. Succeeds, and the crate compiles.
/// - A path-reached item moves while the parent names it: the assist rewrites the reference and the
///   import pass restores the binding, which is why [`refuse_stranded`] says an in-file reference
///   costs nothing. Succeeds.
/// - A member is lifted out of an `impl` while a sibling in that same `impl` calls it: the new module
///   is written *outside* the impl, so from in there the rewritten path never named anything and
///   rust-analyzer will not rename what does not resolve. Refuses.
///
/// Refusing either of the first two would turn working restructures into refusals, which is why this
/// weighs only members of an `impl` the seam cut through.
///
/// The prescription differs from [`refuse_residual_placeholder`]'s deliberately. Reordering is the fix
/// when the stranded reference sits in an already-extracted *module*; it cannot help here, because an
/// `impl` body cannot hold a `mod`, so the sibling can be moved neither out of the way first nor
/// after. The seam has to grow.
fn refuse_impl_sibling_references(items: &[MovedItem]) -> Result<()> {
    let blocked: Vec<String> = items
        .iter()
        .filter(|item| !item.referenced_in_impl_at.is_empty())
        .map(|item| {
            format!(
                "`{}` from line(s) {}",
                item.name,
                item.referenced_in_impl_at
                    .iter()
                    .map(u32::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })
        .collect();

    if blocked.is_empty() {
        return Ok(());
    }

    Err(failure(format!(
        "this seam cuts an `impl` in half, and a member left behind still calls one that would move: \
         {}. The new module is written outside the `impl`, so that call would resolve nowhere and \
         rust-analyzer will not rewrite it. An `impl` body cannot hold a `mod`, so no ordering helps — \
         grow the seam to carry the whole `impl`, or cut it where nothing crosses.",
        blocked.join("; ")
    )))
}

/// Every item a range would relocate, however it is reached.
///
/// The companion to [`path_reached_within`], and deliberately a second traversal rather than a flag on
/// the first. They answer different questions: that one asks which items a *module path* reaches,
/// which is what decides facades and visibility, and it is right to stop above an `impl`. This one
/// asks what physically moves, so it descends through every container — which is the only way an
/// `impl` member is seen at all.
///
/// `within` carries the enclosing containers rather than only the enclosing modules, so an item's
/// entry names the `impl` that holds it. That is what lets a later pass ask whether the seam cut
/// through an `impl` instead of around it.
fn items_relocated_within(symbols: &Value, range: Range) -> Vec<PathReached> {
    let mut found = Vec::new();
    collect_relocated(symbols, range, &[], &mut found);
    found
}

fn collect_relocated(
    symbols: &Value,
    range: Range,
    within: &[String],
    found: &mut Vec<PathReached>,
) {
    for symbol in symbols.as_array().into_iter().flatten() {
        let name = symbol.get("name").and_then(Value::as_str);
        let covered = covers(range, symbol.pointer("/range/start"));

        // A container whose name is not a single identifier — `impl Gauge` — is still a container, so
        // it is skipped as an item and kept as a step in the path.
        if covered {
            if let (Some(item), Some(position)) = (
                name.filter(|name| is_identifier(name)),
                symbol
                    .pointer("/selectionRange/start")
                    .or_else(|| symbol.pointer("/location/range/start")),
            ) {
                found.push(PathReached {
                    name: item.to_string(),
                    within: within.to_vec(),
                    position: position.clone(),
                });
            }
        }

        // Descend whether or not the container itself is covered. A seam that cuts into an `impl`
        // starts *below* the `impl` keyword by construction, so stopping at an uncovered container
        // would walk past every member the range actually holds — which is the only geometry this
        // traversal exists to see.
        if let Some(children) = symbol.get("children") {
            let mut inside = within.to_vec();
            inside.extend(name.map(str::to_owned));
            collect_relocated(children, range, &inside, found);
        }
    }
}

/// The `impl` blocks the range cuts through rather than around, by name as the server reports them.
///
/// An `impl` is cut through when the range reaches some of its members and not others. That is the
/// geometry the extraction cannot survive: the new module is written outside the `impl`, so the
/// members left behind reference a path that never resolved from where they sit.
///
/// Computed from the members alone, because a partially covered block is exactly one with a member
/// the range does not reach — no end position required, and none is reliably reported.
fn impls_cut_through(symbols: &Value, range: Range) -> Vec<String> {
    let mut cut = Vec::new();
    collect_impls_cut_through(symbols, range, &mut cut);
    cut
}

fn collect_impls_cut_through(symbols: &Value, range: Range, cut: &mut Vec<String>) {
    for symbol in symbols.as_array().into_iter().flatten() {
        let children: Vec<&Value> = symbol
            .get("children")
            .and_then(Value::as_array)
            .map(|kids| kids.iter().collect())
            .unwrap_or_default();

        if symbol.get("kind").and_then(Value::as_u64) == Some(SYMBOL_KIND_IMPL) {
            let reached = children
                .iter()
                .filter(|child| covers(range, child.pointer("/range/start")))
                .count();
            if reached > 0 && reached < children.len() {
                if let Some(name) = symbol.get("name").and_then(Value::as_str) {
                    cut.push(name.to_string());
                }
            }
        }

        if let Some(kids) = symbol.get("children") {
            collect_impls_cut_through(kids, range, cut);
        }
    }
}

/// The visibilities the assist widened on members no path-reached survey ever sees.
///
/// `restore_visibility` iterates the items [`path_reached_within`] returned, and that traversal stops
/// above an `impl` — so a relocated `impl` member is neither put back nor named, and lands
/// `pub(crate)` in silence while the schema promises every widening is reported.
///
/// Reported rather than narrowed, deliberately. Narrowing would reintroduce `E0624` for a private
/// method with a sibling-module caller, which is safe today precisely *because* the item stays
/// widened; naming it makes that a decision instead of an accident.
///
/// A member written `pub` or already `pub(crate)` has nothing to answer for, so the visibility each
/// one was written with is compared rather than inferred from the relocated text — which cannot tell a
/// widening from an item that always read that way.
fn impl_widenings(
    source: &[String],
    block: &ModuleBlock,
    members: &[MovedItem],
) -> Vec<VisibilityChange> {
    members
        .iter()
        .filter(|member| member.visibility != "pub" && member.visibility != WIDENED.trim())
        .filter(|member| {
            source[block.opened..block.closed]
                .iter()
                .any(|line| declares_at_widened_visibility(line, &member.name))
        })
        .map(|member| VisibilityChange {
            item: member.name.clone(),
            from: if member.visibility.is_empty() {
                "private".to_string()
            } else {
                member.visibility.clone()
            },
            to: WIDENED.trim().to_string(),
        })
        .collect()
}

/// The `use` lines the parent keeps so paths that reached the relocated items still resolve.
///
/// A glob is one line and legal whatever moved: a glob re-export caps at each item's own visibility
/// rather than failing on a member less visible than itself. A named re-export cannot do that — `pub
/// use` of a `pub(crate)` item is `E0365` — so its names are grouped by the visibility each item was
/// written with, widest first. It names only the items something outside the new module reaches,
/// because re-exporting a helper that travelled with its only caller would make it reachable for
/// nobody and undo the privacy the seam just preserved.
fn facade_lines(module: &str, items: &[MovedItem], kind: Reexport) -> Result<Vec<String>> {
    Ok(match kind {
        Reexport::None => Vec::new(),
        Reexport::Glob => vec![format!("pub use {module}::*;")],
        Reexport::Named => {
            refuse_uncovered_nesting(items)?;
            let reached: Vec<&MovedItem> = items
                .iter()
                .filter(|item| item.reached_from_outside && item.within.is_empty())
                .collect();

            let mut tiers: Vec<&str> = Vec::new();
            for item in &reached {
                if !tiers.contains(&item.visibility.as_str()) {
                    tiers.push(item.visibility.as_str());
                }
            }
            // Widest first, and stable within a tier so the order follows the source.
            tiers.sort_by_key(|tier| match *tier {
                "pub" => 0,
                "" => 2,
                _ => 1,
            });

            tiers
                .into_iter()
                .map(|tier| {
                    let names: Vec<&str> = reached
                        .iter()
                        .filter(|item| item.visibility == tier)
                        .map(|item| item.name.as_str())
                        .collect();
                    let prefix = if tier.is_empty() {
                        String::new()
                    } else {
                        format!("{tier} ")
                    };
                    format!("{prefix}use {module}::{{{}}};", names.join(", "))
                })
                .collect()
        }
    })
}

/// Refuse a named facade for an item whose own module the facade will not carry.
///
/// A nested item is reached through the module holding it, so re-exporting that module keeps
/// `parent::nested::buried` resolving untouched and naming the item as well would only publish
/// `parent::buried` — a path no caller ever used. Naming it flat is worse still: that is the
/// `pub use grouped::{nested, buried};` that earned `E0432` while the run reported success.
///
/// The residual case has no honest line at all: something outside reaches the nested item, nothing
/// reaches the module holding it, so the facade would have to invent a path or drop the reference.
/// Refuse, and say which item and which module, because the fix is a different seam.
fn refuse_uncovered_nesting(items: &[MovedItem]) -> Result<()> {
    let carried: Vec<&str> = items
        .iter()
        .filter(|item| item.reached_from_outside && item.within.is_empty())
        .map(|item| item.name.as_str())
        .collect();

    let uncovered: Vec<String> = items
        .iter()
        .filter(|item| item.reached_from_outside)
        .filter_map(|item| {
            let holder = item.within.first()?;
            (!carried.contains(&holder.as_str()))
                .then(|| format!("`{}` inside `{holder}`", item.name))
        })
        .collect();

    if uncovered.is_empty() {
        return Ok(());
    }

    Err(failure(format!(
        "a named re-export cannot keep these paths resolving: {}. Nothing outside reaches the module \
         holding them, so there is no name the parent could re-export that would cover them. Ask for \
         `reexport: glob`, which re-exports the module too, or cut the seam so the nested item stays \
         behind.",
        uncovered.join("; ")
    )))
}

/// What a `named` re-export that re-exported nothing has to say for itself.
///
/// A seam whose items are all internal legitimately needs no facade, so this is not a refusal — but
/// a developer who asked for a facade and got none has to hear it here rather than find it in the
/// diff.
fn empty_facade_note(module: &str, lines: &[String], kind: Reexport) -> Option<String> {
    (kind == Reexport::Named && lines.is_empty()).then(|| {
        format!(
            "`reexport: named` on `{module}` re-exported nothing: everything the seam moved is \
             reached only from inside it. The facade was asked for and is not there."
        )
    })
}

/// Refuse an extraction whose module name the parent already uses for something else.
///
/// The name is the one piece of text an extraction invents, and it is the only name a facade can
/// collide on: every other name in the module was already unique, and an extraction only moves names
/// out. A `mod report` written beside an existing `pub mod report;` is `E0428`, and nothing in the
/// assist's own output says so — the run reports success and the crate stops compiling.
///
/// Read from the text, so it costs no crate graph and answers before a server is started. A
/// declaration inside the range is not counted: those move into the new module and vacate the name.
fn refuse_module_name_taken(text: &str, module: &str, range: Range) -> Result<()> {
    let retained = outside_the_range(text, range);

    let taken = module_declaration(&retained, module).or_else(|| {
        imported_paths(&retained)
            .into_iter()
            .find(|path| path.rsplit("::").next() == Some(module))
            .map(|path| format!("use {path}"))
    });

    match taken {
        None => Ok(()),
        Some(binding) => Err(failure(format!(
            "`{module}` is already taken in this module by `{binding}`. A second declaration of the \
             name is `E0428` and the assist writes it without complaint, so the run would report \
             success against a crate that no longer compiles. Give the module a different name."
        ))),
    }
}

/// The text with the relocated range blanked out, line numbering preserved.
///
/// An extraction moves names *out*, so a declaration inside the seam vacates the parent and its name
/// is free for the new module to take. Blanking rather than deleting keeps every remaining line where
/// it was, which is what lets the same read serve checks that report a line.
fn outside_the_range(text: &str, range: Range) -> String {
    text.split('\n')
        .enumerate()
        .map(|(index, line)| {
            let number = index as u32 + 1;
            if number >= range.start.line && number <= range.end.line {
                ""
            } else {
                line
            }
        })
        .collect::<Vec<&str>>()
        .join("\n")
}

/// The `mod` declaration of `name` the text carries, if it carries one.
///
/// Lexical and deliberately narrow. A line that merely mentions the name — a comment, a doc comment,
/// an attribute, a call — declares nothing, and a name that is a prefix of another (`report` beside
/// `reporting`) is not it either.
fn module_declaration(text: &str, name: &str) -> Option<String> {
    text.split('\n').map(str::trim).find_map(|line| {
        if line.starts_with("//") || line.starts_with('#') {
            return None;
        }
        let rest = strip_visibility(line).strip_prefix("mod ")?;
        (rest.trim_end_matches([';', '{', ' ']).trim() == name).then(|| line.to_string())
    })
}

/// A declaration with its leading `pub`, `pub(crate)`, `pub(super)` … removed.
fn strip_visibility(declaration: &str) -> &str {
    let visibility = visibility_in(declaration);
    if visibility.is_empty() {
        return declaration;
    }
    declaration[visibility.len()..].trim_start()
}

/// Every path an attribute names by string, with the line it was written on.
///
/// `#[serde(default = "default_extend")]` reaches an item the way a call does, but rust-analyzer
/// answers `textDocument/references` on that item without it: serde builds the call out of the
/// string's *contents*, so the identifier it generates has no span in the source. A seam is
/// therefore free to separate the two, and only the compiler ever says so.
///
/// A `doc` attribute is excluded: its string is prose, and prose that happens to read as a path is
/// not a reference to anything.
fn attribute_path_names(text: &str) -> Vec<(usize, String)> {
    let mut found = Vec::new();

    for (index, line) in text.split('\n').enumerate() {
        let trimmed = line.trim();
        let Some(body) = trimmed
            .strip_prefix("#![")
            .or_else(|| trimmed.strip_prefix("#["))
        else {
            continue;
        };
        if body.starts_with("doc") {
            continue;
        }
        for literal in body.split('"').skip(1).step_by(2) {
            if is_path(literal) {
                found.push((index + 1, literal.to_string()));
            }
        }
    }

    found
}

/// Whether a string literal reads as a Rust path, and so names something rather than saying something.
fn is_path(literal: &str) -> bool {
    !literal.is_empty() && literal.split("::").all(is_identifier)
}

/// Refuse a seam that would separate an attribute's string path from the item it names.
///
/// Only an unqualified name is weighed. A qualified one — `crate::defaults::extend` — resolves the
/// same from either side of the seam, so moving the attribute or the item changes nothing. A bare
/// identifier resolves in the scope the attribute sits in, so putting the two in different modules
/// breaks it, silently, in code no diff shows.
///
/// A name this file does not declare is left alone: it lives in another module, and this seam is not
/// what separates them.
fn refuse_split_attribute_paths(text: &str, range: Range) -> Result<()> {
    let split: Vec<String> = attribute_path_names(text)
        .into_iter()
        .filter(|(_, path)| !path.contains("::"))
        .filter_map(|(line, path)| {
            let declared = declaration_line(text, &path)?;
            let inside = |line: usize| {
                line as u32 >= range.start.line && line as u32 <= range.end.line
            };
            (inside(line) != inside(declared)).then(|| {
                format!("`{path}`, named by the attribute on line {line} and declared on line {declared}")
            })
        })
        .collect();

    if split.is_empty() {
        return Ok(());
    }

    Err(failure(format!(
        "the seam would separate an attribute from the item its string names: {}. The name resolves \
         in the scope the attribute sits in, and no reference query reports the attribute — a macro \
         builds the call out of the string's contents, so the identifier it generates has no span to \
         find. Cut the seam so the two stay together.",
        split.join("; ")
    )))
}

/// The one-based line on which the text declares `name`, if it declares it.
fn declaration_line(text: &str, name: &str) -> Option<usize> {
    const KEYWORDS: [&str; 8] = [
        "fn ", "struct ", "enum ", "trait ", "mod ", "const ", "static ", "type ",
    ];

    text.split('\n').enumerate().find_map(|(index, line)| {
        let trimmed = line.trim();
        if trimmed.starts_with("//") || trimmed.starts_with('#') {
            return None;
        }
        let declaration = strip_visibility(trimmed);
        let rest = KEYWORDS
            .iter()
            .find_map(|keyword| declaration.strip_prefix(keyword))?;
        let declared = rest
            .split(|c: char| !is_identifier_char(c))
            .next()
            .unwrap_or_default();
        (declared == name).then_some(index + 1)
    })
}

/// Insert the facade immediately after the module the assist wrote.
///
/// The module's end is found by indentation rather than by counting braces. A brace inside a string
/// literal is ordinary Rust — `format!("{:.1}", …)` carries a pair — and counting would be at their
/// mercy, while the assist always writes the closing brace alone on a line at the `mod` keyword's own
/// indent. Searching for a marker after the module is no better: an adjacent seam's markers move, and
/// one item's doc comment is routinely a prefix of another's.
fn with_facade(text: &str, module: &str, lines: &[String]) -> Result<String> {
    if lines.is_empty() {
        return Ok(text.to_string());
    }

    let mut source: Vec<String> = text.split('\n').map(str::to_string).collect();
    let block = module_bounds(&source, module)?;

    for (offset, line) in lines.iter().enumerate() {
        source.insert(block.closed + 1 + offset, format!("{}{line}", block.indent));
    }

    Ok(source.join("\n"))
}

/// The lines an inline module opens and closes on, and the indent it sits at.
struct ModuleBlock {
    opened: usize,
    closed: usize,
    indent: String,
}

/// Locate the module the assist wrote.
///
/// By indentation, not by counting braces. A brace inside a string literal is ordinary Rust —
/// `format!("{:.1}", …)` carries a pair — and counting would be at their mercy, while the assist
/// always writes the closing brace alone on a line at the `mod` keyword's own indent. Searching for a
/// marker after the module is no better: an adjacent seam's markers move, and one item's doc comment
/// is routinely a prefix of another's.
fn module_bounds(source: &[String], module: &str) -> Result<ModuleBlock> {
    let header = format!("mod {module}");

    let opened = source
        .iter()
        .position(|line| line.trim_start().starts_with(&header) && line.trim_end().ends_with('{'))
        .ok_or_else(|| {
            failure(format!(
                "rust-analyzer did not write a `{header}` block where one was expected"
            ))
        })?;

    let indent =
        source[opened][..source[opened].len() - source[opened].trim_start().len()].to_string();
    let closing = format!("{indent}}}");
    let closed = source
        .iter()
        .skip(opened + 1)
        .position(|line| line.trim_end() == closing)
        .map(|offset| opened + 1 + offset)
        .ok_or_else(|| failure(format!("`{header}` is never closed at its own indent")))?;

    Ok(ModuleBlock {
        opened,
        closed,
        indent,
    })
}

/// The visibility the assist widens everything it relocates to.
const WIDENED: &str = "pub(crate) ";

/// Item keywords a declaration's name can follow.
const ITEM_KEYWORDS: [&str; 8] = [
    "fn", "struct", "enum", "mod", "const", "static", "type", "trait",
];

/// Put back the visibility an item was written with, wherever nothing outside the new module needs it
/// widened, and report every widening that has to stand.
///
/// The assist rewrites everything it relocates to `pub(crate)`. Across one real restructure that
/// widened 56 items, six of them fields kept private specifically to force mutation through a single
/// method — an invariant the compiler had been enforcing, left as a comment that then contradicted the
/// code. Nothing is narrowed here that a reference still needs, so a seam that co-locates a private
/// helper with its only caller keeps the privacy, and a seam that does not says so out loud.
fn restore_visibility(
    text: &str,
    module: &str,
    items: &[MovedItem],
) -> Result<(String, Vec<VisibilityChange>)> {
    let mut source: Vec<String> = text.split('\n').map(str::to_string).collect();
    let block = module_bounds(&source, module)?;
    let mut report = Vec::new();

    for item in items {
        // The assist never narrows, so an item written `pub` has nothing to answer for.
        if item.visibility == "pub" {
            continue;
        }

        // Only inside the module the assist just wrote. A same-named item elsewhere in the file is a
        // different item, and rewriting its visibility would be a change nobody asked for.
        let Some(index) = source[block.opened..block.closed]
            .iter()
            .position(|line| declares_at_widened_visibility(line, &item.name))
            .map(|offset| block.opened + offset)
        else {
            continue;
        };

        if item.reached_from_outside {
            report.push(VisibilityChange {
                item: item.name.clone(),
                from: if item.visibility.is_empty() {
                    "private".to_string()
                } else {
                    item.visibility.clone()
                },
                to: "pub(crate)".to_string(),
            });
            continue;
        }

        let line = source[index].clone();
        let indent = &line[..line.len() - line.trim_start().len()];
        let rest = line.trim_start().strip_prefix(WIDENED).unwrap_or_default();
        source[index] = if item.visibility.is_empty() {
            format!("{indent}{rest}")
        } else {
            format!("{indent}{} {rest}", item.visibility)
        };
    }

    Ok((source.join("\n"), report))
}

/// Whether `line` declares `name` at the visibility the assist widened it to.
fn declares_at_widened_visibility(line: &str, name: &str) -> bool {
    let Some(rest) = line.trim_start().strip_prefix(WIDENED) else {
        return false;
    };

    let tokens: Vec<&str> = rest
        .split(|character: char| !is_identifier_char(character))
        .filter(|token| !token.is_empty())
        .collect();

    tokens
        .windows(2)
        .any(|pair| ITEM_KEYWORDS.contains(&pair[0]) && pair[1] == name)
}

/// One contiguous run of `before` lines and what replaces it. `from` and `to` index `before`.
struct ChangedRegion<'a> {
    from: usize,
    to: usize,
    lines: Vec<&'a str>,
}

/// The regions in which `before` and `after` differ.
///
/// The lines the two versions share are what make the result useful: they are the coordinates a
/// later anchor can still be translated into, so the diff recognises as many of them as it can
/// rather than settling for a common prefix and suffix.
fn changed_regions<'a>(before: &[&'a str], after: &[&'a str]) -> Vec<ChangedRegion<'a>> {
    let mut regions = Vec::new();
    let mut from = 0;
    let mut cursor = 0;

    for run in common_runs(before, after) {
        if run.before > from || run.after > cursor {
            regions.push(ChangedRegion {
                from,
                to: run.before,
                lines: after[cursor..run.after].to_vec(),
            });
        }
        from = run.before + run.length;
        cursor = run.after + run.length;
    }

    if from < before.len() || cursor < after.len() {
        regions.push(ChangedRegion {
            from,
            to: before.len(),
            lines: after[cursor..].to_vec(),
        });
    }

    regions
}

/// A maximal run of lines the two versions agree on, at `before` and `after` respectively.
struct CommonRun {
    before: usize,
    after: usize,
    length: usize,
}

/// The common runs of a shortest edit script, by Myers' algorithm.
///
/// The search walks diagonals of the edit graph, recording the furthest point reached on each one at
/// every edit distance; the recorded frontiers are then walked backwards to recover the path, whose
/// diagonal moves are exactly the lines the two versions share.
///
/// A path is always found within `n + m` edits — delete everything, then insert everything — so the
/// loop cannot fall through, and the empty case is answered before the frontier is sized.
fn common_runs(before: &[&str], after: &[&str]) -> Vec<CommonRun> {
    let n = before.len() as isize;
    let m = after.len() as isize;
    let max = n + m;
    if max == 0 {
        return Vec::new();
    }

    let offset = max;
    let at = |k: isize| (k + offset) as usize;
    let mut frontier = vec![0isize; (2 * max + 1) as usize];
    let mut trace: Vec<Vec<isize>> = Vec::new();

    for d in 0..=max {
        trace.push(frontier.clone());

        let mut k = -d;
        while k <= d {
            // The furthest-reaching path on this diagonal arrives either from the one below or from
            // the one above; `&&` short-circuits so the out-of-range neighbour is never read.
            let go_down = k == -d || (k != d && frontier[at(k - 1)] < frontier[at(k + 1)]);
            let mut x = if go_down {
                frontier[at(k + 1)]
            } else {
                frontier[at(k - 1)] + 1
            };
            let mut y = x - k;

            while x < n && y < m && before[x as usize] == after[y as usize] {
                x += 1;
                y += 1;
            }

            frontier[at(k)] = x;

            if x >= n && y >= m {
                return backtrack(&trace, n, m, offset);
            }
            k += 2;
        }
    }

    unreachable!("a shortest edit script always exists within {max} edits")
}

/// Walks the recorded frontiers backwards, collecting the diagonal moves of the shortest path.
fn backtrack(trace: &[Vec<isize>], n: isize, m: isize, offset: isize) -> Vec<CommonRun> {
    let at = |k: isize| (k + offset) as usize;
    let mut runs = Vec::new();
    let mut x = n;
    let mut y = m;

    for step in (1..trace.len()).rev() {
        let frontier = &trace[step];
        let d = step as isize;
        let k = x - y;
        let go_down = k == -d || (k != d && frontier[at(k - 1)] < frontier[at(k + 1)]);
        let previous_k = if go_down { k + 1 } else { k - 1 };
        let previous_x = frontier[at(previous_k)];
        let previous_y = previous_x - previous_k;

        let mut length = 0;
        while x > previous_x && y > previous_y {
            x -= 1;
            y -= 1;
            length += 1;
        }
        if length > 0 {
            runs.push(CommonRun {
                before: x as usize,
                after: y as usize,
                length,
            });
        }

        x = previous_x;
        y = previous_y;
    }

    if x > 0 {
        runs.push(CommonRun {
            before: 0,
            after: 0,
            length: x as usize,
        });
    }

    runs.reverse();
    runs
}

fn uri_of(path: &Path) -> String {
    format!("file://{}", path.display())
}

fn failure(reason: impl Into<String>) -> RestructureError {
    RestructureError::MalformedPlan(reason.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_token_type_position_from_the_servers_legend() {
        let handshake = json!({
            "capabilities": {
                "semanticTokensProvider": {
                    "legend": { "tokenTypes": ["comment", "keyword", UNRESOLVED_TOKEN] }
                }
            }
        });

        assert_eq!(token_type_index(&handshake, UNRESOLVED_TOKEN), Some(2));
    }

    /// The legend is per-server, so a missing entry has to be reported rather than guessed at.
    #[test]
    fn reports_a_token_type_the_legend_does_not_carry() {
        let handshake = json!({
            "capabilities": { "semanticTokensProvider": { "legend": { "tokenTypes": ["keyword"] } } }
        });

        assert_eq!(token_type_index(&handshake, UNRESOLVED_TOKEN), None);
    }

    #[test]
    fn ignores_a_handshake_that_declares_no_semantic_tokens() {
        assert_eq!(
            token_type_index(&json!({ "capabilities": {} }), UNRESOLVED_TOKEN),
            None
        );
    }

    /// Each token is five integers — line delta, start delta, length, type, modifiers — and the
    /// start delta is relative to the token before it only while both sit on the same line.
    #[test]
    fn decodes_positions_from_the_deltas_between_tokens() {
        let text = "let a = BTreeMap::new();\nlet b = HashSet::new();\n";
        let tokens = json!({ "data": [1, 8, 7, 3, 0, 0, 0, 0, 9, 0] });

        let found = unresolved_in(&tokens, 3, text);

        assert_eq!(found.len(), 1);
        assert_eq!(found[0].text, "HashSet");
        assert_eq!(found[0].position, json!({ "line": 1, "character": 8 }));
    }

    #[test]
    fn keeps_only_the_tokens_of_the_requested_type() {
        let text = "let a = BTreeMap::new();\n";
        let tokens = json!({ "data": [0, 4, 1, 9, 0, 0, 4, 8, 3, 0] });

        let found = unresolved_in(&tokens, 3, text);

        assert_eq!(found.len(), 1);
        assert_eq!(found[0].text, "BTreeMap");
    }

    /// A token can describe a version of the file this client no longer holds; a span that runs off
    /// the end of the text it is read against is not a name to ask the server about.
    #[test]
    fn drops_a_token_whose_span_falls_outside_the_text() {
        let tokens = json!({ "data": [0, 4, 40, 3, 0] });

        assert!(unresolved_in(&tokens, 3, "let a = 1;\n").is_empty());
    }

    #[test]
    fn expands_a_grouped_use_into_one_path_per_name() {
        let paths = imported_paths("use lopdf::{Document, Object, dictionary};\n");

        assert_eq!(
            paths,
            ["lopdf::Document", "lopdf::Object", "lopdf::dictionary"]
        );
    }

    #[test]
    fn expands_a_group_nested_inside_another() {
        let paths = imported_paths("use std::{collections::{HashMap, HashSet}, sync::Mutex};\n");

        assert_eq!(
            paths,
            [
                "std::collections::HashMap",
                "std::collections::HashSet",
                "std::sync::Mutex"
            ]
        );
    }

    #[test]
    fn reads_a_group_that_spans_several_lines() {
        let paths = imported_paths("use crate::core::{\n    Alpha,\n    Beta,\n};\n");

        assert_eq!(paths, ["crate::core::Alpha", "crate::core::Beta"]);
    }

    #[test]
    fn binds_the_module_itself_for_a_self_member() {
        assert_eq!(
            imported_paths("use lopdf::{self, Object};\n"),
            ["lopdf", "lopdf::Object"]
        );
    }

    /// A glob says nothing about which names it carries, so it contributes none — the cost is a
    /// refusal on an ambiguous name, never a wrongly chosen import.
    #[test]
    fn contributes_nothing_for_a_glob() {
        assert!(imported_paths("use lopdf::*;\n").is_empty());
    }

    #[test]
    fn reads_the_path_an_alias_binds_rather_than_the_alias() {
        assert_eq!(
            imported_paths("use lopdf::Object as PdfObject;\n"),
            ["lopdf::Object"]
        );
    }

    /// A line quoting an import in prose is not one, and neither is the tail of the statement
    /// before it: the tree is read from the declaration's own line.
    #[test]
    fn ignores_an_import_written_inside_a_comment() {
        let paths =
            imported_paths("/// Callers write `use lopdf::Object;` themselves.\nlet a = 1;\n");

        assert!(paths.is_empty());
    }

    #[test]
    fn applies_the_only_import_the_server_offers() {
        let offered = ["Import `lopdf::Object`"];

        assert_eq!(choose_import("", &offered), Some("Import `lopdf::Object`"));
    }

    #[test]
    fn settles_a_contested_name_on_the_path_the_file_already_imports() {
        let offered = ["Import `js_sys::Object`", "Import `lopdf::Object`"];

        let chosen = choose_import("use lopdf::{Document, Object};\n", &offered);

        assert_eq!(chosen, Some("Import `lopdf::Object`"));
    }

    #[test]
    fn settles_nothing_when_the_file_imports_neither_candidate() {
        let offered = ["Import `js_sys::Object`", "Import `lopdf::Object`"];

        assert_eq!(choose_import("use std::sync::Mutex;\n", &offered), None);
    }

    #[test]
    fn settles_nothing_when_the_file_imports_both_candidates() {
        let offered = ["Import `js_sys::Object`", "Import `lopdf::Object`"];
        let text = "use js_sys::Object;\nuse lopdf::Object;\n";

        assert_eq!(choose_import(text, &offered), None);
    }

    #[test]
    fn reads_no_names_from_a_response_carrying_no_data() {
        assert!(unresolved_in(&json!({}), 3, "let a = 1;\n").is_empty());
    }

    /// A hunk is expressed as a half-open range of whole lines: column one at both ends, so the
    /// ledger's line arithmetic and the applier's byte spans agree on what it covers.
    #[test]
    fn starts_and_ends_every_hunk_at_column_one() {
        let edits = minimal_edits("a\nb\nc\n", "a\nB\nc\n");

        assert!(edits
            .iter()
            .all(|edit| edit.range.start.col == 1 && edit.range.end.col == 1));
    }

    /// The defect that forced one plan per Rust extraction: an operation changing two distant places
    /// reported a single span covering everything between, and the ledger then correctly refused
    /// every anchor in the untouched middle.
    #[test]
    fn reports_two_distant_changes_as_two_hunks() {
        let before = "one\ntwo\nthree\nfour\nfive\nsix\n";
        let after = "ONE\ntwo\nthree\nfour\nfive\nSIX\n";

        assert_eq!(minimal_edits(before, after).len(), 2);
    }

    #[test]
    fn leaves_the_lines_between_two_hunks_addressable() {
        let before = "one\ntwo\nthree\nfour\nfive\nsix\n";
        let after = "ONE\ntwo\nthree\nfour\nfive\nSIX\n";

        let edits = minimal_edits(before, after);

        // Lines 2..5 are shared, so no hunk may cover them.
        assert!(edits
            .iter()
            .all(|edit| edit.range.end.line <= 2 || edit.range.start.line >= 6));
    }

    /// `line_count` counts newlines while the replaced span is `end.line - start.line`, so a
    /// replacement missing its terminator shifts every later anchor by one.
    #[test]
    fn terminates_every_replacement_with_a_newline() {
        let edits = minimal_edits("a\nb\nc\n", "a\nB\nB2\nc\n");

        assert!(edits
            .iter()
            .all(|edit| edit.new_text.is_empty() || edit.new_text.ends_with('\n')));
    }

    #[test]
    fn reports_a_pure_deletion_as_an_empty_replacement() {
        let edits = minimal_edits("a\nb\nc\n", "a\nc\n");

        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].new_text, "");
        assert_eq!(edits[0].range.start.line, 2);
        assert_eq!(edits[0].range.end.line, 3);
    }

    #[test]
    fn reports_nothing_for_two_identical_texts() {
        assert!(minimal_edits("a\nb\n", "a\nb\n").is_empty());
    }

    /// The path `convert_change` takes for a file the assist brought into existence, where the
    /// baseline is the empty string rather than anything on disk.
    #[test]
    fn expresses_a_created_file_as_an_insertion_at_line_one() {
        let edits = minimal_edits("", "mod counting;\n");

        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].range.start.line, 1);
        assert_eq!(edits[0].range.end.line, 1);
        assert_eq!(edits[0].new_text, "mod counting;\n");
    }

    /// `apply_text_edits` replaces bottom-up in pre-edit coordinates and `record_changes` rebases
    /// within the batch; both break if two hunks overlap.
    #[test]
    fn emits_hunks_that_never_overlap() {
        let before = "a\nb\nc\nd\ne\nf\ng\nh\n";
        let after = "a\nB\nc\nd\nE\nf\ng\nH\n";

        let edits = minimal_edits(before, after);

        for pair in edits.windows(2) {
            assert!(pair[0].range.end.line <= pair[1].range.start.line);
        }
    }

    #[test]
    fn rebuilds_the_later_text_from_the_hunks_it_reported() {
        let before = "alpha\nbeta\ngamma\ndelta\nepsilon\n";
        let after = "alpha\nBETA\ngamma\ndelta\nEPSILON\nomega\n";

        let mut lines: Vec<String> = before.split('\n').map(str::to_string).collect();
        for edit in minimal_edits(before, after).into_iter().rev() {
            let start = edit.range.start.line as usize - 1;
            let end = edit.range.end.line as usize - 1;
            let replacement: Vec<String> = if edit.new_text.is_empty() {
                Vec::new()
            } else {
                edit.new_text
                    .strip_suffix('\n')
                    .unwrap_or(&edit.new_text)
                    .split('\n')
                    .map(str::to_string)
                    .collect()
            };
            lines.splice(start..end, replacement);
        }

        assert_eq!(lines.join("\n"), after);
    }

    #[test]
    fn refuses_a_rename_that_left_the_placeholder_behind() {
        let original = "fn a() {}\n";
        let produced = "mod grouped { fn a() -> modname::T {} }\n";

        assert!(refuse_residual_placeholder(original, produced, "modname").is_err());
    }

    #[test]
    fn names_every_line_the_placeholder_survived_on() {
        let original = "fn a() {}\nfn b() {}\n";
        let produced = "fn a() -> modname::T {}\nfn b() -> modname::U {}\n";

        let message = refuse_residual_placeholder(original, produced, "modname")
            .unwrap_err()
            .to_string();

        assert!(message.contains('1'), "{message}");
        assert!(message.contains('2'), "{message}");
    }

    /// Counting against zero rather than against the text as it arrived would make any file that
    /// happens to contain the identifier unrefactorable.
    #[test]
    fn accepts_a_document_that_already_carried_the_placeholder_identifier() {
        let original = "fn modname() {}\n";
        let produced = "mod grouped {\n    fn modname() {}\n}\n";

        assert!(refuse_residual_placeholder(original, produced, "modname").is_ok());
    }

    #[test]
    fn accepts_a_result_the_rename_reached_everywhere() {
        let original = "fn a() -> T {}\n";
        let produced = "mod grouped { fn a() -> grouped::T {} }\n";

        assert!(refuse_residual_placeholder(original, produced, "modname").is_ok());
    }

    /// `fun_name` inside `fun_names` is a different identifier, and refusing on it would reject code
    /// that is perfectly well formed.
    #[test]
    fn reads_past_a_longer_identifier_the_placeholder_is_only_a_prefix_of() {
        assert!(placeholder_sites("let fun_names = 1;\n", "fun_name").is_empty());
    }

    #[test]
    fn reads_the_placeholder_where_a_path_separator_follows_it() {
        assert_eq!(placeholder_sites("a\nmodname::T\n", "modname"), [2]);
    }

    /// The shape rust-analyzer actually answers `documentSymbol` with, captured from the server: an
    /// `impl` block is `Object` (19) carrying `Method` (6) children, and an inline `mod` is
    /// `Module` (2) carrying whatever it holds.
    fn outline() -> Value {
        json!([
            {
                "name": "loose", "kind": 12,
                "range": { "start": { "line": 0, "character": 0 } },
                "selectionRange": { "start": { "line": 0, "character": 3 } }
            },
            {
                "name": "impl Gauge", "kind": 19,
                "range": { "start": { "line": 1, "character": 0 } },
                "selectionRange": { "start": { "line": 1, "character": 5 } },
                "children": [{
                    "name": "doubled", "kind": 6,
                    "range": { "start": { "line": 2, "character": 4 } },
                    "selectionRange": { "start": { "line": 2, "character": 11 } }
                }]
            },
            {
                "name": "nested", "kind": 2,
                "range": { "start": { "line": 3, "character": 0 } },
                "selectionRange": { "start": { "line": 3, "character": 8 } },
                "children": [{
                    "name": "buried", "kind": 12,
                    "range": { "start": { "line": 4, "character": 4 } },
                    "selectionRange": { "start": { "line": 4, "character": 11 } }
                }]
            }
        ])
    }

    fn whole_file() -> Range {
        Range {
            start: Position { line: 1, col: 1 },
            end: Position { line: 99, col: 1 },
        }
    }

    fn names_of(found: &[PathReached]) -> Vec<&str> {
        found.iter().map(|item| item.name.as_str()).collect()
    }

    /// Where the range holds each item it found, keyed by name, for the nesting the facade needs.
    fn within_of<'a>(found: &'a [PathReached], name: &str) -> &'a [String] {
        found
            .iter()
            .find(|item| item.name == name)
            .map(|item| item.within.as_slice())
            .expect("the outline carries that item")
    }

    /// The too-loose half of the old check: nothing nested was ever inspected, so an item one module
    /// down could be relocated with a reference elsewhere left pointing at nothing.
    #[test]
    fn descends_into_the_children_of_a_module_the_range_covers() {
        assert!(names_of(&path_reached_within(&outline(), whole_file())).contains(&"buried"));
    }

    /// Resolution goes through the type, so the `impl` can move anywhere in the crate and no caller
    /// changes. Checking the method would refuse a move that is always safe.
    #[test]
    fn classifies_a_method_inside_an_impl_as_reached_through_its_type() {
        assert!(!names_of(&path_reached_within(&outline(), whole_file())).contains(&"doubled"));
    }

    #[test]
    fn reads_every_item_a_module_path_can_name() {
        assert_eq!(
            names_of(&path_reached_within(&outline(), whole_file())),
            ["loose", "nested", "buried"]
        );
    }

    #[test]
    fn reads_nothing_from_a_range_covering_no_symbol() {
        let range = Range {
            start: Position { line: 40, col: 1 },
            end: Position { line: 50, col: 1 },
        };

        assert!(path_reached_within(&outline(), range).is_empty());
    }

    #[test]
    fn reads_the_visibility_an_item_was_written_with() {
        assert_eq!(visibility_in("pub fn "), "pub");
        assert_eq!(visibility_in("pub(crate) fn "), "pub(crate)");
        assert_eq!(visibility_in("pub(super) fn "), "pub(super)");
        assert_eq!(
            visibility_in("    pub(in crate::core) fn "),
            "pub(in crate::core)"
        );
        assert_eq!(visibility_in("fn "), "");
    }

    /// `publish` starts with `pub` and is not a visibility; reading one out of it would widen an item
    /// that never asked for it.
    #[test]
    fn reads_no_visibility_out_of_an_identifier_that_merely_starts_with_pub() {
        assert_eq!(visibility_in("publish "), "");
    }

    #[test]
    fn reads_the_visibility_at_the_position_the_server_reported() {
        let text = "pub(crate) fn tier(reading: f64) -> u32 {\n";
        let position = json!({ "line": 0, "character": 14 });

        assert_eq!(visibility_at(text, &position), "pub(crate)");
    }

    fn moved(name: &str, visibility: &str, outside: bool) -> MovedItem {
        MovedItem {
            name: name.to_string(),
            visibility: visibility.to_string(),
            within: Vec::new(),
            stranded_in: Vec::new(),
            reached_from_outside: outside,
            referenced_in_impl_at: Vec::new(),
        }
    }

    /// A moved item the range holds inside an inline module of its own, which is what a facade
    /// cannot name flat.
    fn moved_within(name: &str, module: &str, outside: bool) -> MovedItem {
        MovedItem {
            name: name.to_string(),
            visibility: "pub".to_string(),
            within: vec![module.to_string()],
            stranded_in: Vec::new(),
            reached_from_outside: outside,
            referenced_in_impl_at: Vec::new(),
        }
    }

    #[test]
    fn writes_one_glob_reexport_for_the_module_it_grouped() {
        let items = [moved("render", "pub", true), moved("normalise", "", false)];

        assert_eq!(
            facade_lines("rendering", &items, Reexport::Glob).unwrap(),
            ["pub use rendering::*;"]
        );
    }

    /// `pub use` of a `pub(crate)` item is `E0365`, so the names cannot share one declaration.
    #[test]
    fn groups_a_named_reexport_by_the_visibility_each_item_was_written_with() {
        let items = [
            moved("render", "pub", true),
            moved("normalise", "", false),
            moved("clamp", "", true),
            moved("tier", "pub(crate)", true),
        ];

        assert_eq!(
            facade_lines("rendering", &items, Reexport::Named).unwrap(),
            [
                "pub use rendering::{render};",
                "pub(crate) use rendering::{tier};",
                "use rendering::{clamp};",
            ]
        );
    }

    /// Naming a helper that travelled with its only caller would make it reachable again for nobody,
    /// undoing the privacy the seam just preserved.
    #[test]
    fn names_only_the_items_something_outside_the_module_reaches() {
        let items = [
            moved("render", "pub", true),
            moved("normalise", "pub", false),
        ];

        assert_eq!(
            facade_lines("rendering", &items, Reexport::Named).unwrap(),
            ["pub use rendering::{render};"]
        );
    }

    #[test]
    fn writes_nothing_when_no_facade_was_asked_for() {
        assert!(
            facade_lines("rendering", &[moved("render", "pub", true)], Reexport::None)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn puts_the_facade_immediately_after_the_module_it_belongs_to() {
        let text = "before\nmod rendering {\n    fn a() {}\n}\nafter\n";

        let result =
            with_facade(text, "rendering", &["pub use rendering::*;".to_string()]).unwrap();

        assert_eq!(
            result,
            "before\nmod rendering {\n    fn a() {}\n}\npub use rendering::*;\nafter\n"
        );
    }

    /// A brace inside a string literal is ordinary Rust, and counting braces would end the module in
    /// the wrong place. The closing brace at the keyword's own indent does not have that problem.
    #[test]
    fn finds_the_end_of_a_module_whose_body_carries_braces_inside_a_string() {
        let text = "mod rendering {\n    fn a() -> String { format!(\"{:.1}\", 1.0) }\n}\ntail\n";

        let result =
            with_facade(text, "rendering", &["pub use rendering::*;".to_string()]).unwrap();

        assert!(
            result.contains("}\npub use rendering::*;\ntail"),
            "{result}"
        );
    }

    #[test]
    fn refuses_to_place_a_facade_beside_a_module_that_was_never_written() {
        assert!(with_facade(
            "fn a() {}\n",
            "rendering",
            &["pub use rendering::*;".to_string()]
        )
        .is_err());
    }

    #[test]
    fn reads_a_declaration_the_assist_widened() {
        assert!(declares_at_widened_visibility(
            "    pub(crate) fn render(x: f64) {",
            "render"
        ));
        assert!(declares_at_widened_visibility(
            "pub(crate) struct Gauge {",
            "Gauge"
        ));
    }

    #[test]
    fn reads_no_declaration_out_of_a_doc_comment_mentioning_the_name() {
        assert!(!declares_at_widened_visibility(
            "/// pub(crate) fn render is nice",
            "render"
        ));
    }

    #[test]
    fn reads_no_declaration_where_the_name_is_only_a_parameter() {
        assert!(!declares_at_widened_visibility(
            "pub(crate) fn other(render: f64) {",
            "render"
        ));
    }

    /// The seam the lessons call for: a private helper that travels with its only caller keeps the
    /// privacy the compiler was enforcing.
    #[test]
    fn narrows_a_relocated_item_back_to_the_visibility_it_had() {
        let widened = "mod rendering {\n    pub(crate) fn normalise(v: f64) -> f64 { v }\n}\n";

        let (restored, report) =
            restore_visibility(widened, "rendering", &[moved("normalise", "", false)]).unwrap();

        assert!(
            restored.contains("    fn normalise(v: f64) -> f64 { v }"),
            "{restored}"
        );
        assert!(report.is_empty());
    }

    #[test]
    fn keeps_the_widening_of_an_item_something_outside_the_module_reaches() {
        let widened = "mod rendering {\n    pub(crate) fn clamp(v: f64) -> f64 { v }\n}\n";

        let (restored, report) =
            restore_visibility(widened, "rendering", &[moved("clamp", "", true)]).unwrap();

        assert!(restored.contains("pub(crate) fn clamp"), "{restored}");
        assert_eq!(report.len(), 1);
        assert_eq!(report[0].item, "clamp");
        assert_eq!(report[0].from, "private");
        assert_eq!(report[0].to, "pub(crate)");
    }

    #[test]
    fn puts_a_scoped_visibility_back_as_it_was_written() {
        let widened = "mod rendering {\n    pub(crate) fn helper(v: f64) -> f64 { v }\n}\n";

        let (restored, _) = restore_visibility(
            widened,
            "rendering",
            &[moved("helper", "pub(super)", false)],
        )
        .unwrap();

        assert!(restored.contains("    pub(super) fn helper"), "{restored}");
    }

    /// The assist never narrows, so an item written `pub` is already as it was and has nothing to
    /// report.
    #[test]
    fn leaves_an_item_that_was_already_public_alone() {
        let text = "mod rendering {\n    pub fn render(v: f64) -> f64 { v }\n}\n";

        let (restored, report) =
            restore_visibility(text, "rendering", &[moved("render", "pub", true)]).unwrap();

        assert_eq!(restored, text);
        assert!(report.is_empty());
    }

    #[test]
    fn names_the_item_and_the_file_whose_reference_would_be_stranded() {
        let mut item = moved("scaled_label", "pub", true);
        item.stranded_in = vec!["src/lib.rs".to_string()];

        let message = refuse_stranded(&[item]).unwrap_err().to_string();

        assert!(message.contains("scaled_label"), "{message}");
        assert!(message.contains("src/lib.rs"), "{message}");
    }

    #[test]
    fn refuses_nothing_when_every_reference_travels_with_the_items() {
        assert!(refuse_stranded(&[moved("normalise", "", false)]).is_ok());
    }

    /// A same-named item outside the module the assist wrote is a different item, and rewriting its
    /// visibility would be a change nobody asked for.
    #[test]
    fn leaves_a_same_named_declaration_outside_the_module_alone() {
        let text = "pub(crate) fn helper(v: f64) -> f64 { v }\n\nmod rendering {\n    pub(crate) fn helper(v: f64) -> f64 { v }\n}\n";

        let (restored, _) =
            restore_visibility(text, "rendering", &[moved("helper", "", false)]).unwrap();

        let lines: Vec<&str> = restored.split('\n').collect();
        assert_eq!(lines[0], "pub(crate) fn helper(v: f64) -> f64 { v }");
        assert_eq!(lines[3], "    fn helper(v: f64) -> f64 { v }");
    }

    #[test]
    fn refuses_to_restore_visibility_inside_a_module_that_was_never_written() {
        assert!(restore_visibility("fn a() {}\n", "rendering", &[moved("a", "", false)]).is_err());
    }

    /// The exact signature CI produced: rust-analyzer offered the extraction before inference was
    /// ready and filled the return type with placeholders, giving E0121 and a crate that cannot build.
    #[test]
    fn refuses_an_extraction_whose_return_type_was_never_inferred() {
        let text = "fn compute_spread(sample: &Sample) -> (_, _) {\n    (1.0, 2.0)\n}\n";

        assert!(refuse_inferred_placeholder(text, "fn compute_spread").is_err());
    }

    #[test]
    fn names_the_signature_it_refused() {
        let text = "fn compute_spread(sample: &Sample) -> (_, _) {\n";

        let message = refuse_inferred_placeholder(text, "fn compute_spread")
            .unwrap_err()
            .to_string();

        assert!(message.contains("-> (_, _)"), "{message}");
    }

    #[test]
    fn accepts_a_signature_whose_types_were_inferred() {
        let text = "fn compute_spread(sample: &Sample) -> (f64, f64) {\n    (1.0, 2.0)\n}\n";

        assert!(refuse_inferred_placeholder(text, "fn compute_spread").is_ok());
    }

    /// `fun_name` and `var_name` carry an underscore without being one, and the placeholder names this
    /// backend renames are exactly those — so a naive substring check would refuse every extraction.
    #[test]
    fn reads_no_placeholder_type_out_of_an_identifier_containing_an_underscore() {
        assert!(!carries_placeholder_type(
            "fn fun_name(sample: &Sample) -> f64 {"
        ));
        assert!(!carries_placeholder_type(
            "let var_name = highest - lowest;"
        ));
    }

    #[test]
    fn reads_a_placeholder_type_standing_alone() {
        assert!(carries_placeholder_type("fn f() -> _ {"));
        assert!(carries_placeholder_type("fn f(value: _) -> f64 {"));
        assert!(carries_placeholder_type("fn f() -> Vec<_> {"));
    }

    fn block_of(text: &str) -> (Vec<String>, ModuleBlock) {
        let source: Vec<String> = text.split('\n').map(str::to_string).collect();
        let block = module_bounds(&source, "moved").expect("a `mod moved` block");
        (source, block)
    }

    fn unresolved_at(line: usize, text: &str) -> UnresolvedName {
        UnresolvedName {
            text: text.to_string(),
            position: json!({ "line": line, "character": 0 }),
        }
    }

    /// `use super::new_with_config;` for an associated function: the server reports the name
    /// unresolved on the very line that binds it, which is the whole evidence needed.
    #[test]
    fn drops_an_assist_import_that_binds_nothing() {
        let (source, block) = block_of(
            "mod moved {\n    use super::new_with_config;\n    use super::Manager;\n    fn go() {}\n}\n",
        );

        let kept = without_dead_imports(&source, &block, &[unresolved_at(1, "new_with_config")]);

        assert!(!kept.iter().any(|line| line.contains("new_with_config")));
        assert!(kept.iter().any(|line| line.contains("use super::Manager;")));
    }

    /// A grouped `use` carries names beyond the one in question, so it is never the line to drop —
    /// the bare duplicate is, whether it sits above the group or below it.
    #[test]
    fn drops_a_bare_duplicate_of_a_grouped_import_written_above_it() {
        let (source, block) = block_of(
            "mod moved {\n    use global_context_api;\n    use crate::core::{export_cursor, global_context_api};\n    fn go() {}\n}\n",
        );

        let kept = without_dead_imports(&source, &block, &[]);

        assert_eq!(
            kept[1],
            "    use crate::core::{export_cursor, global_context_api};"
        );
        assert!(kept.iter().any(|line| line.contains("export_cursor")));
    }

    #[test]
    fn drops_a_bare_duplicate_of_a_grouped_import_written_below_it() {
        let (source, block) = block_of(
            "mod moved {\n    use crate::core::{a, global_context_api};\n    use global_context_api;\n    fn go() {}\n}\n",
        );

        let kept = without_dead_imports(&source, &block, &[]);

        assert_eq!(
            kept.iter()
                .filter(|line| line.contains("global_context_api"))
                .count(),
            1
        );
        assert!(kept[1].contains('{'));
    }

    #[test]
    fn drops_the_second_of_two_bare_imports_of_one_name() {
        let (source, block) = block_of(
            "mod moved {\n    use super::Manager;\n    use other::Manager;\n    fn go() {}\n}\n",
        );

        let kept = without_dead_imports(&source, &block, &[]);

        assert_eq!(kept[1], "    use super::Manager;");
        assert!(!kept.iter().any(|line| line.contains("other::Manager")));
    }

    /// A line that resolves stays, however unused it looks: dropping a trait import would break
    /// method resolution with nothing in the diff to explain it.
    #[test]
    fn keeps_an_import_the_server_resolves() {
        let (source, block) =
            block_of("mod moved {\n    use std::fmt::Write;\n    fn go() {}\n}\n");

        assert_eq!(without_dead_imports(&source, &block, &[]), source);
    }

    /// The parent's own imports are not the assist's to judge, and a name unresolved out there says
    /// nothing about the block being repaired.
    #[test]
    fn leaves_imports_outside_the_block_alone() {
        let (source, block) = block_of("use super::stale;\n\nmod moved {\n    fn go() {}\n}\n");

        let kept = without_dead_imports(&source, &block, &[unresolved_at(0, "stale")]);

        assert_eq!(kept, source);
    }

    /// The three shapes of useless import rust-analyzer offers, each of which was written into a
    /// real restructure as a `use` line that did not compile.
    ///
    /// An associated function is reached through its type, so no `use` binds it — and the server
    /// offers `Import super::new_with_config` for `NativePDFContextManager::new_with_config` as
    /// readily as for a free item.
    #[test]
    fn treats_a_name_after_a_path_qualifier_as_unimportable() {
        let text = "    None => NativePDFContextManager::new_with_config(cfg),";
        let at = json!({ "line": 0, "character": text.find("new_with_config").unwrap() });

        assert!(reached_through_qualifier(text, &at));
    }

    #[test]
    fn treats_a_field_or_method_after_a_dot_as_unimportable() {
        let text = "    let count = manager.context_count();";
        let at = json!({ "line": 0, "character": text.find("context_count").unwrap() });

        assert!(reached_through_qualifier(text, &at));
    }

    /// A range is not a qualifier: the name after `..` is an ordinary expression, and a constant
    /// there is as importable as one anywhere else.
    #[test]
    fn treats_a_name_after_a_range_as_importable() {
        let text = "    for index in 0..MAX_METADATA_SIZE_BYTES {";
        let at = json!({ "line": 0, "character": text.find("MAX_METADATA").unwrap() });

        assert!(!reached_through_qualifier(text, &at));
    }

    #[test]
    fn treats_a_bare_name_as_importable() {
        let text = "    let manager_mutex = GLOBAL_CONTEXT_MANAGER";
        let at = json!({ "line": 0, "character": text.find("GLOBAL").unwrap() });

        assert!(!reached_through_qualifier(text, &at));
    }

    /// The second binding of a name is `E0252` however well its path reads, so a name the module
    /// already imports is never the import the move lost.
    #[test]
    fn finds_a_name_the_module_already_binds() {
        let text = "mod chunked_export {\n    use crate::core::context_manager::global_context_api;\n\n    fn go() {}\n}\n";

        assert!(already_bound(text, "chunked_export", "global_context_api").unwrap());
    }

    /// Read over the whole file this would be true of every name a sibling module imports, and the
    /// guard would skip imports the moved code genuinely needs.
    #[test]
    fn ignores_a_name_only_a_sibling_module_binds() {
        let text = "mod shading_pdf {\n    use lopdf::Object;\n}\n\nmod xobject_pdf {\n    fn go() {}\n}\n";

        assert!(!already_bound(text, "xobject_pdf", "Object").unwrap());
    }

    /// The file's own imports still pick first — that is `choose_import` — but the rest stay
    /// available, because the first choice is now verified rather than trusted.
    #[test]
    fn tries_the_path_the_file_points_at_first_then_the_others() {
        let text = "use crate::core::context_manager::global_context_api;\n";
        let offered = vec![
            "Import super::super::GLOBAL_CONTEXT_MANAGER".to_string(),
            "Import crate::core::context_manager::global_context_api".to_string(),
        ];

        assert_eq!(
            import_order(text, &offered),
            Some(vec![
                "Import crate::core::context_manager::global_context_api",
                "Import super::super::GLOBAL_CONTEXT_MANAGER",
            ])
        );
    }

    #[test]
    fn offers_the_only_path_there_is() {
        let offered = vec!["Import super::GLOBAL_CONTEXT_MANAGER".to_string()];

        assert_eq!(
            import_order("", &offered),
            Some(vec!["Import super::GLOBAL_CONTEXT_MANAGER"])
        );
    }

    /// Two paths and nothing to choose between them is still a refusal: verification can say whether
    /// a path resolves, never whether it is the one the moved code meant.
    #[test]
    fn refuses_to_order_paths_nothing_settles() {
        let offered = vec![
            "Import alpha::Shape".to_string(),
            "Import beta::Shape".to_string(),
        ];

        assert_eq!(import_order("", &offered), None);
    }

    #[test]
    fn accepts_a_document_that_declares_nothing_matching() {
        assert!(refuse_inferred_placeholder("fn other() -> _ {\n", "fn compute_spread").is_ok());
    }

    /// `use gauging::{impl Gauge};` is what collecting it produced: a syntax error, plus `E0252` on
    /// the type the assist had already moved. An `impl` is reached through its type and no module
    /// path can spell it, so there is nothing here for a facade or the stranded check to weigh.
    #[test]
    fn does_not_treat_an_impl_block_as_an_item_a_module_path_can_name() {
        assert!(!names_of(&path_reached_within(&outline(), whole_file())).contains(&"impl Gauge"));
    }

    /// `buried` lives at `nested::buried`, and a facade that wrote its name flat produced
    /// `pub use grouped::{nested, buried};` — `E0432`. The module it sits in travels with it.
    #[test]
    fn records_the_module_that_holds_an_item_one_level_down() {
        let found = path_reached_within(&outline(), whole_file());

        assert_eq!(within_of(&found, "buried"), ["nested"]);
        assert!(within_of(&found, "loose").is_empty());
    }

    /// Re-exporting the module keeps `parent::nested::buried` resolving exactly as it did, so naming
    /// the item as well would only publish a path — `parent::buried` — that no caller ever used.
    #[test]
    fn omits_an_item_the_reexport_of_its_own_module_already_carries() {
        let items = [
            moved("nested", "pub", true),
            moved_within("buried", "nested", true),
        ];

        assert_eq!(
            facade_lines("grouped", &items, Reexport::Named).unwrap(),
            ["pub use grouped::{nested};"]
        );
    }

    /// The residual case: something outside reaches the nested item while nothing reaches the module
    /// holding it, so no line the facade can write keeps the old path resolving. Refuse rather than
    /// write one that does not.
    #[test]
    fn refuses_a_named_facade_for_a_nested_item_no_reexport_would_cover() {
        let items = [
            moved("nested", "pub", false),
            moved_within("buried", "nested", true),
        ];

        let message = facade_lines("grouped", &items, Reexport::Named)
            .unwrap_err()
            .to_string();

        assert!(message.contains("buried"), "{message}");
        assert!(message.contains("nested"), "{message}");
    }

    /// A glob re-exports the module too, so the nested path keeps resolving with nothing special done.
    #[test]
    fn writes_a_glob_reexport_for_a_seam_that_carries_a_nested_item() {
        let items = [
            moved("nested", "pub", true),
            moved_within("buried", "nested", true),
        ];

        assert_eq!(
            facade_lines("grouped", &items, Reexport::Glob).unwrap(),
            ["pub use grouped::*;"]
        );
    }

    /// Asking for a facade and silently getting none is the one outcome the report channel exists to
    /// prevent.
    #[test]
    fn reports_a_named_facade_that_had_nothing_to_reexport() {
        let note = empty_facade_note("grouped", &[], Reexport::Named)
            .expect("a named facade that wrote nothing has something to say");

        assert!(note.contains("grouped"), "{note}");
    }

    #[test]
    fn says_nothing_about_a_facade_that_did_reexport_something() {
        let lines = ["pub use grouped::{nested};".to_string()];

        assert_eq!(empty_facade_note("grouped", &lines, Reexport::Named), None);
    }

    /// A seam that asked for no facade got what it asked for; there is nothing to report.
    #[test]
    fn says_nothing_about_a_seam_that_asked_for_no_facade() {
        assert_eq!(empty_facade_note("grouped", &[], Reexport::None), None);
    }

    /// `mod report {` written beside `pub mod report;` is `E0428`, and the run reported success.
    #[test]
    fn refuses_a_module_name_the_parent_already_declares() {
        let text = "pub mod report;\n\npub fn min_of() {}\npub fn max_of() {}\n";

        let message = refuse_module_name_taken(text, "report", lines(3, 4))
            .unwrap_err()
            .to_string();

        assert!(message.contains("report"), "{message}");
    }

    /// An extraction moves names *out*, so a name declared inside the seam vacates the parent and is
    /// free for the module to take. Refusing it would reject a correct plan.
    #[test]
    fn accepts_a_module_name_only_the_relocated_items_declare() {
        let text = "pub fn other() {}\n\nfn report() {}\npub fn max_of() {}\n";

        assert!(refuse_module_name_taken(text, "report", lines(3, 4)).is_ok());
    }

    /// An import binds the name in the same namespace a `mod` declaration would.
    #[test]
    fn refuses_a_module_name_an_import_already_binds() {
        let text = "use crate::report;\n\npub fn min_of() {}\n";

        assert!(refuse_module_name_taken(text, "report", lines(3, 3)).is_err());
    }

    /// A comment quoting a declaration declares nothing, and refusing on one would block a name the
    /// parent never used.
    #[test]
    fn reads_no_declaration_out_of_a_comment_mentioning_the_name() {
        let text = "/// see `pub mod report;` for the rest\n\npub fn min_of() {}\n";

        assert!(refuse_module_name_taken(text, "report", lines(3, 3)).is_ok());
    }

    /// The whole-line range these checks are handed, given as one-based line numbers.
    fn lines(start: u32, end: u32) -> Range {
        Range {
            start: Position {
                line: start,
                col: 1,
            },
            end: Position { line: end, col: 1 },
        }
    }

    /// Captured from the server: `title` arrives only with `begin`, so a `report` that follows has to
    /// be told what work it belongs to.
    #[test]
    fn reads_a_progress_line_from_a_work_done_notification() {
        let mut chatter = ServerChatter::default();

        chatter.absorb(&json!({
            "method": "$/progress",
            "params": {
                "token": "rustAnalyzer/cachePriming",
                "value": { "kind": "begin", "title": "Priming caches", "cancellable": false }
            }
        }));
        let line = chatter
            .absorb(&json!({
                "method": "$/progress",
                "params": {
                    "token": "rustAnalyzer/cachePriming",
                    "value": { "kind": "report", "message": "20/28 (serde_core)", "percentage": 71 }
                }
            }))
            .expect("a progress report says something worth printing");

        assert!(line.contains("Priming caches"), "{line}");
        assert!(line.contains("20/28 (serde_core)"), "{line}");
        assert!(line.contains("71"), "{line}");
    }

    /// The timeout message names where the server got to, which is only possible if the pump kept it.
    #[test]
    fn keeps_the_last_progress_line_for_the_message_a_timeout_needs() {
        let mut chatter = ServerChatter::default();

        chatter.absorb(&json!({
            "method": "$/progress",
            "params": { "token": "t", "value": { "kind": "begin", "title": "Fetching" } }
        }));

        assert_eq!(chatter.last.as_deref(), Some("Fetching"));
    }

    #[test]
    fn reads_quiescence_from_a_server_status_notification() {
        let mut chatter = ServerChatter::default();

        chatter.absorb(&json!({
            "method": "experimental/serverStatus",
            "params": { "health": "ok", "quiescent": false }
        }));
        assert!(!chatter.quiescent);

        chatter.absorb(&json!({
            "method": "experimental/serverStatus",
            "params": { "health": "ok", "quiescent": true }
        }));
        assert!(chatter.quiescent);
    }

    /// An answer to a request is not progress, and printing one would bury the lines that are.
    #[test]
    fn says_nothing_about_a_message_that_answers_a_request() {
        let mut chatter = ServerChatter::default();

        assert_eq!(chatter.absorb(&json!({ "id": 7, "result": null })), None);
    }

    /// serde builds the call out of the string's contents, so `textDocument/references` on the helper
    /// does not report this site — verified against the server. Reading it lexically is the only way
    /// the seam ever hears about it.
    #[test]
    fn reads_the_path_an_attribute_names_by_string() {
        let text = "pub struct Settings {\n    #[serde(default = \"default_extend\")]\n    pub extend: f64,\n}\n";

        assert_eq!(
            attribute_path_names(text),
            [(2usize, "default_extend".to_string())]
        );
    }

    /// A doc attribute's string is prose. Reading a path out of it would refuse seams over a sentence.
    #[test]
    fn reads_no_path_out_of_a_doc_attribute() {
        assert!(attribute_path_names("#[doc = \"see the notes\"]\npub fn a() {}\n").is_empty());
    }

    /// The failure this pins landed inside serde's generated code, where no reference query looks.
    #[test]
    fn refuses_a_seam_that_separates_an_attribute_from_the_item_it_names() {
        let text = attributed();

        let message = refuse_split_attribute_paths(&text, lines(1, 5))
            .unwrap_err()
            .to_string();

        assert!(message.contains("default_extend"), "{message}");
    }

    #[test]
    fn accepts_a_seam_that_carries_an_attribute_and_the_item_it_names_together() {
        assert!(refuse_split_attribute_paths(&attributed(), lines(1, 9)).is_ok());
    }

    /// The other direction: the attribute stays and the helper it names is the thing that moves.
    #[test]
    fn refuses_a_seam_that_moves_the_item_an_attribute_names_away() {
        assert!(refuse_split_attribute_paths(&attributed(), lines(7, 9)).is_err());
    }

    /// A struct whose field defaults through a helper named by string, with the helper below it.
    fn attributed() -> String {
        [
            "#[derive(Deserialize)]", // 1
            "pub struct Settings {",  // 2
            "    #[serde(default = \"default_extend\")]",
            "    pub extend: f64,",             // 4
            "}",                                // 5
            "",                                 // 6
            "pub fn default_extend() -> f64 {", // 7
            "    1.5",                          // 8
            "}",                                // 9
            "",
        ]
        .join("\n")
    }

    /// The traversal that has to see an `impl` member, because nothing else does.
    ///
    /// `outline()` carries `impl Gauge` with a `doubled` child, which is the shape the server really
    /// reports — an `impl` as `Object` (19) holding a `Method` (6).
    #[test]
    fn reports_an_impl_member_among_the_items_a_range_relocates() {
        let found = items_relocated_within(&outline(), whole_file());

        assert!(
            names_of(&found).contains(&"doubled"),
            "{:?}",
            names_of(&found)
        );
    }

    /// The other half of the contract, and the reason these are two traversals rather than one flag.
    /// A method is reached through its type, so no module path names it and no facade can either —
    /// which is exactly why the path-reached survey must go on ignoring it.
    #[test]
    fn still_leaves_an_impl_member_out_of_what_a_module_path_reaches() {
        let found = path_reached_within(&outline(), whole_file());

        assert!(
            !names_of(&found).contains(&"doubled"),
            "{:?}",
            names_of(&found)
        );
    }

    /// Measured, not assumed: lifting one method out of an `impl` while a sibling inside that same
    /// `impl` calls it is the one geometry of three that cannot be repaired. The new module is written
    /// outside the impl, so the call resolves nowhere and the rename cannot reach it.
    #[test]
    fn refuses_a_seam_whose_impl_sibling_still_references_what_it_moves() {
        let mut item = moved("dial_offset", "", true);
        item.referenced_in_impl_at = vec![51];

        assert!(refuse_impl_sibling_references(&[item]).is_err());
    }

    #[test]
    fn names_the_member_and_the_line_its_impl_sibling_calls_it_from() {
        let mut item = moved("dial_offset", "", true);
        item.referenced_in_impl_at = vec![51];

        let message = refuse_impl_sibling_references(&[item])
            .unwrap_err()
            .to_string();

        assert!(message.contains("dial_offset"), "{message}");
        assert!(message.contains("51"), "{message}");
    }

    /// The prescription is not the one the placeholder refusal gives. An `impl` body cannot hold a
    /// `mod`, so the sibling can be moved neither out of the way first nor after; the seam has to grow.
    #[test]
    fn says_an_impl_sibling_seam_must_grow_rather_than_be_reordered() {
        let mut item = moved("dial_offset", "", true);
        item.referenced_in_impl_at = vec![51];

        let message = refuse_impl_sibling_references(&[item])
            .unwrap_err()
            .to_string();

        assert!(message.contains("grow"), "{message}");
        assert!(
            !message.to_lowercase().contains("reorder the plan"),
            "{message}"
        );
    }

    /// A whole `impl` moving while the parent calls its methods succeeds today and compiles. Refusing
    /// it would turn working work into a refusal, which is the most expensive way to be wrong.
    #[test]
    fn accepts_a_seam_no_impl_sibling_references() {
        assert!(refuse_impl_sibling_references(&[moved("doubled", "pub", true)]).is_ok());
    }

    /// The widening the report has never mentioned, because `restore_visibility` iterates only what
    /// the path-reached survey returned and that survey stops above an `impl`.
    #[test]
    fn reports_a_private_impl_member_the_assist_widened() {
        let (source, block) = block_of(
            "mod moved {\n    impl Dial {\n        pub(crate) fn dial_offset(&self) -> f64 {\n            0.0\n        }\n    }\n}\n",
        );

        let widened = impl_widenings(&source, &block, &[moved("dial_offset", "", true)]);

        assert_eq!(
            widened
                .iter()
                .map(|change| change.item.as_str())
                .collect::<Vec<_>>(),
            ["dial_offset"]
        );
    }

    /// Already `pub` before the move, so the assist widened nothing and there is nothing to answer for.
    #[test]
    fn says_nothing_about_an_impl_member_that_was_already_public() {
        let (source, block) = block_of(
            "mod moved {\n    impl Dial {\n        pub fn bearing(&self) -> f64 {\n            0.0\n        }\n    }\n}\n",
        );

        assert!(impl_widenings(&source, &block, &[moved("bearing", "pub", true)]).is_empty());
    }

    /// `offset_of` counted `char`s where `position_at` counted bytes, so the two disagreed on any
    /// line carrying a character outside the BMP — and rust-analyzer, never told which unit to use,
    /// answered in UTF-16 code units, a third figure again.
    ///
    /// Byte 17 is the `"` closing the literal: 13 bytes of prefix plus the emoji's four. Counting
    /// `char`s, 17 ran off the end of a 16-character line and resolved to the newline instead.
    #[test]
    fn round_trips_a_position_that_follows_an_astral_character() {
        let text = "let label = \"\u{1F600}\";\nlet next = 1;\n";
        let point = LspPoint {
            line: 0,
            character: 17,
        };

        let offset = offset_of(text, point);

        assert_eq!(
            position_at(text, offset),
            json!({ "line": point.line, "character": point.character })
        );
    }

    /// The emoji's own first byte, which both units have to agree names the character's start.
    #[test]
    fn resolves_the_offset_an_astral_character_begins_at() {
        let text = "let label = \"\u{1F600}\";\n";

        let offset = offset_of(
            text,
            LspPoint {
                line: 0,
                character: 13,
            },
        );

        assert_eq!(&text[offset..offset + 4], "\u{1F600}");
    }

    /// Cause one of two: the leftover sits inside a module the plan already extracted, where the
    /// rewritten path never resolved. Reordering the plan is the fix, and this is the case the current
    /// single message describes correctly.
    #[test]
    fn prescribes_reordering_when_the_leftover_sits_in_an_extracted_module() {
        let original = "mod grouped {\n    fn a() -> T {}\n}\n";
        let produced = "mod grouped {\n    fn a() -> modname::T {}\n}\n";

        let message = refuse_residual_placeholder(original, produced, "modname")
            .unwrap_err()
            .to_string();

        assert!(
            message.contains("before the items that reference it"),
            "{message}"
        );
    }

    /// Cause two, which the single message names nobody and prescribes the wrong fix for: the leftover
    /// sits inside an `impl`, where no reordering can put the definition first.
    #[test]
    fn prescribes_growing_the_seam_when_the_leftover_sits_in_an_impl() {
        let original =
            "impl Dial {\n    fn bearing(&self) -> f64 {\n        self.dial_offset()\n    }\n}\n";
        let produced = "impl Dial {\n    fn bearing(&self) -> f64 {\n        modname::dial_offset(self)\n    }\n}\n";

        let message = refuse_residual_placeholder(original, produced, "modname")
            .unwrap_err()
            .to_string();

        assert!(message.contains("grow"), "{message}");
        assert!(
            !message.contains("before the items that reference it"),
            "{message}"
        );
    }

    /// The tail every Rust split leaves. An empty group is the one part of it that is decidable by
    /// looking: it binds nothing, so removing it cannot change what resolves.
    #[test]
    fn drops_a_use_declaration_whose_group_the_assist_hollowed_out() {
        let text = "use std::sync::{};\nfn tally() {}\n";

        assert_eq!(without_hollow_imports(text), "fn tally() {}\n");
    }

    #[test]
    fn drops_a_hollow_group_that_still_holds_whitespace() {
        assert!(binds_nothing("use std::sync::{ };"));
        assert!(binds_nothing("pub use crate::report::{};"));
    }

    /// A `use` that binds something is left alone however unused it looks. Dropping a trait import
    /// breaks method resolution with no diagnostic pointing at the removal.
    #[test]
    fn keeps_every_use_declaration_that_binds_a_name() {
        assert!(!binds_nothing("use std::sync::{Arc};"));
        assert!(!binds_nothing("use std::sync::Arc;"));
        assert!(!binds_nothing(
            "use std::collections::{BTreeMap, BTreeSet};"
        ));
        assert!(!binds_nothing("use std::fmt::{self};"));
    }

    /// Not a `use` at all, and a line that merely mentions braces is not an import.
    #[test]
    fn keeps_a_line_that_is_not_an_import() {
        assert!(!binds_nothing("fn empty() -> Set {}"));
        assert!(!binds_nothing("let refused = HashMap::new();"));
    }

    /// A substring search finds `mod ranking` when asked for `mod rank`, and the file-extraction
    /// assist would then move the wrong declaration — silently, because both are real modules.
    #[test]
    fn finds_the_module_declaration_and_not_a_longer_name_starting_with_it() {
        let text = "mod ranking {\n}\nmod rank {\n}\n";

        let caret = caret_at_module(text, "rank").unwrap();

        assert_eq!(caret.start.line, 3);
    }

    #[test]
    fn finds_a_declaration_that_is_the_only_one() {
        let caret = caret_at_module("pub mod counting {\n}\n", "counting").unwrap();

        assert_eq!((caret.start.line, caret.start.col), (1, 5));
    }

    /// The name is a prefix of the only candidate, so there is nothing to move out and saying so
    /// beats moving the wrong module.
    #[test]
    fn refuses_when_only_a_longer_name_matches() {
        assert!(caret_at_module("mod ranking {\n}\n", "rank").is_err());
    }

    #[test]
    fn reads_past_a_declaration_whose_name_merely_ends_with_the_one_sought() {
        assert_eq!(whole_word("mod subrank {", "mod rank"), None);
    }

    /// A 600s stall at `discovering sysroot` (Falcon e34fef02) is indistinguishable in a CI log
    /// from a pinned-and-healthy server. This line is what separates the causes.
    #[test]
    fn describes_what_rust_analyzer_was_launched_against() {
        let root = tempfile::tempdir().unwrap();
        let bin = root.path().join("toolchains/1.93.1/bin");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::write(bin.join("cargo"), "").unwrap();

        let described = describe_server_environment(Path::new("/ra/rust-analyzer"), "1.93.1", &bin);

        assert!(described.contains("RUSTUP_TOOLCHAIN=1.93.1"));
        assert!(described.contains("cargo=real"));
        assert!(
            described.contains("rustc=missing"),
            "a proxy-only rustc must be visible, not implied: {described}"
        );
        assert!(described.contains("rust-src=absent"));
    }

    #[test]
    fn reports_rust_src_once_the_sysroot_sources_are_present() {
        let root = tempfile::tempdir().unwrap();
        let prefix = root.path().join("toolchains/1.93.1");
        std::fs::create_dir_all(prefix.join("lib/rustlib/src/rust/library")).unwrap();
        std::fs::create_dir_all(prefix.join("bin")).unwrap();

        let described = describe_server_environment(
            Path::new("/ra/rust-analyzer"),
            "1.93.1",
            &prefix.join("bin"),
        );

        assert!(described.contains("rust-src=present"));
    }
}

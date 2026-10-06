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
use crate::crate_move::{self, ItemReferences, ModuleReferences, Reference};
use crate::edit::{
    FileEdit, Position, Range, Resolution, TextEdit, VisibilityChange, WorkspaceEdit,
};
use crate::item_anchor::{unlowered_item_anchor, ItemAtResolver, ItemResolver};
use crate::plan::{Anchor, Reexport, RefactorKind, RefactorOp};
use crate::registry::{Language, LanguageBackend, Workspace};
use crate::spawn_record::{purpose, SpawnRecorder};
use crate::{RestructureError, Result};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tddy_lsp::client::LspClient;
use tokio_util::sync::CancellationToken;

mod chatter;
mod documents;
mod early_return;
mod escaping_types;
mod impl_seam;
mod imports;
mod inline_paths;
mod introduced;
mod item_move;
mod item_path;
mod module_reparent;
mod nested_modules;
mod prelude_shadow;
mod readiness;
mod relative_visibility;
mod selection;
mod signature;
mod signature_rewrites;

pub use chatter::ServerChatter;

use early_return::refuse_early_returns;

use impl_seam::refuse_impl_sibling_references;
use imports::names_bound;
use introduced::{introduced_by, Introduced};

/// LSP `SymbolKind::Object` — how rust-analyzer reports an `impl` block. Its members are reached
/// through the type, never through a module path, which is why a seam may move a whole `impl` freely
/// and may not cut one in half.
const SYMBOL_KIND_IMPL: u64 = 19;

/// LSP `SymbolKind::Module` — the only nesting a module path grows through. Verified against
/// rust-analyzer's own `documentSymbol`, which reports an `impl` block as `Object` (19) carrying
/// `Method` (6) children, and an inline `mod` as `Module` (2).
const SYMBOL_KIND_MODULE: u64 = 2;

const SUPPORTED: [RefactorKind; 22] = [
    RefactorKind::ExtractMethod,
    RefactorKind::ExtractVariable,
    RefactorKind::ExtractModule,
    RefactorKind::ExtractModuleToFile,
    RefactorKind::ExtractTrait,
    RefactorKind::InlineMethod,
    RefactorKind::RenameSymbol,
    RefactorKind::MoveModuleToCrate,
    RefactorKind::MoveClusterToCrate,
    RefactorKind::MoveTestBinaryToCrate,
    RefactorKind::MoveItem,
    RefactorKind::ReparentModule,
    RefactorKind::RemoveUnusedParam,
    RefactorKind::ConvertTupleReturnToStruct,
    RefactorKind::ChangeParamType,
    RefactorKind::AddParam,
    RefactorKind::ReorderParams,
    RefactorKind::ChangeReturnType,
    RefactorKind::AddCallArg,
    RefactorKind::RemoveCallArg,
    RefactorKind::ChangeCallArg,
    RefactorKind::ReorderCallArgs,
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
            let updated = lsp_edits::apply_lsp_edit(&original, edits_in(change)?);
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
pub fn client_capabilities() -> Value {
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
pub fn server_settings() -> Value {
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
    /// The fixed name the assist writes, or `None` when the server names the symbol itself and it is
    /// found by what the assist added ([`introduced::introduced_by`]).
    name: Option<&'static str>,
}

impl Placeholder {
    /// Whether the declaration is an item signature, where `_` is not a type (`E0121`). A `let` may
    /// carry one (`let v: Vec<_> = …`), and rust-analyzer's initializer may too (`collect::<Vec<_>>()`).
    fn declares_a_signature(&self) -> bool {
        self.keyword != "let"
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
                name: Some("fun_name"),
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
                name: None,
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
                name: Some("modname"),
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
                name: Some("NewTrait"),
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
        // Offered with the caret on the parameter's name, which is why the caret is placed there.
        RefactorKind::RemoveUnusedParam => Some(Assist {
            title: "remove unused parameter",
            kinds: &["refactor"],
            at_caret: true,
            placeholder: None,
            multi_file: true,
            needs_inference: false,
            relocates_items: false,
        }),
        // The struct it writes is named after the function; `multi_file_assist` finds that name by
        // what the assist added and has the server rename it, so there is no fixed placeholder.
        RefactorKind::ConvertTupleReturnToStruct => Some(Assist {
            title: "convert tuple return type to tuple struct",
            kinds: &["refactor.rewrite"],
            at_caret: true,
            placeholder: None,
            multi_file: true,
            needs_inference: false,
            relocates_items: false,
        }),
        _ => None,
    }
}

mod return_type;

/// rust-analyzer answers `codeAction` with an empty list until it has finished loading the crate
/// graph, so a request that needs the graph is retried at this cadence until it is answered.
const INDEXING_POLL: Duration = Duration::from_secs(2);
const SETTLE_POLL: Duration = Duration::from_millis(200);

/// How often a wait looks at its cancellation token while it is sleeping between polls.
///
/// The poll intervals above are the cadence the *server* is asked again at; this is the cadence the
/// *caller* is listened to at. Keeping the two apart is what lets a cancelled wait unwind promptly
/// without asking a loading server more often than it deserves.
const CANCEL_CHECK: Duration = Duration::from_millis(100);

/// The semantic-token type rust-analyzer gives an identifier it cannot resolve. It is an extension
/// to the standard legend, and the only way this client learns which names a moved item has lost
/// without waiting on a `cargo check` it would otherwise have to run per pass.
const UNRESOLVED_TOKEN: &str = "unresolvedReference";

/// The prefix of the code action that adds a `use` declaration. Its sibling `Qualify as …` fixes
/// the same diagnostic by rewriting the reference instead, which is not what an extraction wants.
const IMPORT_TITLE: &str = "Import ";

/// How many times the extraction will ask for one more import.
///
/// Each pass restores a single name and routinely resolves several others that were unresolved only
/// through it. A backstop rather than the anti-spin guarantee: a server that keeps offering an
/// import which changes nothing is already stopped by the occurrence check — an import must reduce
/// that name's unresolved occurrences or the operation refuses — and by the `unimportable` list,
/// which never re-asks a name whose every offered path failed.
///
/// 64 was chosen when the premise was that "the count needed is far below this". That is false for
/// a large module: relocating a 6,484-line `impl` of a generated gRPC trait needs one import per
/// distinct proto type it names, which runs past a hundred, and the run then failed on the bound
/// rather than on anything wrong with the plan.
const IMPORT_PASSES: usize = 512;

/// LSP `ContentModified`. The server is still catching up with a document change and asks the
/// client to re-issue, which is what the specification prescribes rather than treating it as fatal.
const CONTENT_MODIFIED: i64 = -32801;
const CONTENT_MODIFIED_RETRIES: u32 = 30;

pub struct RustBackend {
    binary: PathBuf,
    cargo_home: PathBuf,
    rustup_home: PathBuf,
    server: Option<server_process::Server>,
    /// When set, LSP traffic goes through an existing client instead of a spawned server.
    bridge: Option<LspClientBridge>,
    next_id: u64,
    /// What the running server was launched against, for the indexing-timeout message.
    environment: String,
    /// What the server has said while this client was waiting on an answer.
    chatter: ServerChatter,
    /// How a wait learns that its caller has stopped waiting.
    ///
    /// This is the only thing that ends a wait other than the server becoming ready. There is no
    /// budget: a number this library guessed is not evidence about the server, and a run that
    /// raised one still met a ceiling derived from it. The token is checked *inside* the poll
    /// loops, beside each sleep, because those loops are synchronous and run under
    /// `spawn_blocking` — dropping the calling future stops nothing at all.
    cancel: CancellationToken,
    /// Where a progress line goes. Every other consequence of an operation travels back to the
    /// caller inside a [`Resolution`], but progress happens *while* a call is in flight and has
    /// nowhere to wait — so it needs a sink rather than a return value. It stays a sink rather than
    /// a `println!` because this library is not the only possible front end: anything that speaks a
    /// protocol on stdout, a persistent server most obviously, would have its stream corrupted by an
    /// engine writing progress into it. Silent by default, and the binary is what makes it visible.
    progress: ProgressSink,
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
    /// The documents this backend has opened and not yet closed. See [`documents`].
    opened: Vec<String>,
    /// The workspace root a self-spawned server was started in. A bridged client carries its own.
    root: Option<PathBuf>,
    /// Where the language server this backend starts itself is recorded, when a run asks for it.
    spawns: SpawnRecorder,
}

/// The default progress sink: a library that was not asked to report says nothing.
/// Where a progress line goes.
///
/// A boxed sink rather than a `fn` pointer because a function pointer cannot capture: a host
/// serving several callers at once needs each caller's progress to reach *that* caller, and a bare
/// `fn` has nowhere to put the channel it would have to write to. Shared rather than owned because
/// the same sink is handed to a backend and to the runner around it.
pub type ProgressSink = std::sync::Arc<dyn Fn(&str) + Send + Sync>;

/// A sink that drops every line — the default, so a library front end is silent unless it asks.
pub fn discard() -> ProgressSink {
    std::sync::Arc::new(discard_line)
}

/// The no-op a `fn`-pointer sink defaults to. `trace` is still a bare pointer: it has one
/// destination per front end, not one per caller, so it needs nothing a pointer cannot carry.
fn discard_line(_line: &str) {}

mod server_process;
#[cfg(test)]
use server_process::binds_nothing;
#[cfg(test)]
use server_process::without_hollow_imports;

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
            cancel: CancellationToken::new(),
            progress: discard(),
            trace: discard_line,
            indexed: false,
            unresolved_token: None,
            doc_version: 1,
            claimed: Vec::new(),
            opened: Vec::new(),
            root: None,
            spawns: SpawnRecorder::discard(),
        }
    }

    /// Send progress somewhere. Without this the indexing wait is silent, which is the state the
    /// field report spent two hours in.
    pub fn with_progress(mut self, sink: ProgressSink) -> Self {
        self.progress = sink;
        self
    }

    /// Hand the backend the token that says when its caller has stopped waiting.
    ///
    /// This replaces the budgets. A server is not refused for taking longer than a number this
    /// library guessed; it is waited for until the caller gives up, and the caller is the only one
    /// who knows when that is. The token is checked inside the poll loops rather than awaited,
    /// because those loops are synchronous: this backend runs under `spawn_blocking`, where
    /// dropping the calling future stops nothing.
    pub fn with_cancellation(mut self, cancel: CancellationToken) -> Self {
        self.cancel = cancel.clone();
        // The bridge needs it too, and for the harder half: the poll loops check the token between
        // requests, and only the bridge can end one that is already in flight. A bridge left on
        // the token it was built with would leave every such request unreachable by this run.
        if let Some(bridge) = &mut self.bridge {
            bridge.set_cancellation(cancel);
        }
        self
    }

    /// Record the language server this backend starts itself, through `spawns`.
    pub fn with_spawn_recorder(mut self, spawns: SpawnRecorder) -> Self {
        self.spawns = spawns;
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
    ///
    /// `cancel` is the caller's own token where it has one — a host serving requests takes it from
    /// the task it is serving. `None` says nothing but readiness will end a wait, which is what a
    /// single-shot caller whose process *is* the operation means; [`RustBackend::with_cancellation`]
    /// attaches one to a backend built elsewhere.
    pub fn from_lsp_client(
        client: Arc<LspClient>,
        cancel: Option<CancellationToken>,
        progress: ProgressSink,
    ) -> Self {
        Self {
            binary: PathBuf::new(),
            cargo_home: PathBuf::new(),
            rustup_home: PathBuf::new(),
            server: None,
            bridge: Some(LspClientBridge::new(
                client,
                cancel.clone().unwrap_or_default(),
            )),
            next_id: 0,
            environment: String::from("external tddy-lsp client"),
            chatter: ServerChatter::default(),
            cancel: cancel.unwrap_or_default(),
            progress,
            trace: discard_line,
            indexed: false,
            unresolved_token: None,
            doc_version: 1,
            claimed: Vec::new(),
            opened: Vec::new(),
            root: None,
            spawns: SpawnRecorder::discard(),
        }
    }

    /// The workspace root this backend's server is rooted at, which a workspace-relative path in a
    /// plan is read against.
    ///
    /// A bridged client says so itself, having been initialized against it; a self-spawned server
    /// is rooted wherever [`Self::start`] last started it.
    fn workspace_root(&self) -> Result<PathBuf> {
        if let Some(bridge) = &self.bridge {
            return path_of(bridge.root_uri());
        }
        self.root.clone().ok_or_else(|| {
            failure("this backend has no workspace root: no server has been started against one")
        })
    }

    fn take_id(&mut self) -> u64 {
        self.next_id += 1;
        self.next_id
    }

    /// Sleep until the server is worth asking again, unless the caller has stopped waiting.
    ///
    /// Returns whether waiting may continue. The cancellation check is *here*, beside the sleep,
    /// rather than left to an await point in a caller: every wait in this backend is synchronous
    /// and runs inside `spawn_blocking`, so a dropped future leaves the closure sleeping on.
    fn keep_waiting(&self, poll: Duration) -> bool {
        let until = Instant::now() + poll;
        loop {
            if self.cancel.is_cancelled() {
                return false;
            }
            let remaining = until.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return true;
            }
            std::thread::sleep(remaining.min(CANCEL_CHECK));
        }
    }

    /// The refusal for a wait that ended before the server was ready.
    ///
    /// Where the index got to is carried rather than dropped, because a server that stalled at 12%
    /// and one that was nearly done want opposite responses from whoever reads this.
    fn incomplete_index(&self, waited: Duration) -> RestructureError {
        RestructureError::IndexingIncomplete {
            seconds: waited.as_secs(),
            last: self.chatter.how_far(),
            environment: self.environment.clone(),
        }
    }

    /// Start rust-analyzer and complete the initialize handshake, once per run.
    fn start(&mut self, root: &Path) -> Result<()> {
        (self.progress)("starting rust-analyzer session");
        self.root = Some(root.to_path_buf());
        if let Some(bridge) = &self.bridge {
            // The handshake was someone else's, so the one thing that cannot be assumed is the
            // unit its columns are in. A server left on the LSP default counts utf-16 code
            // units while this client counts bytes, and the two agree on every line until one
            // carries a character outside ASCII — at which point every column is silently
            // wrong rather than refused.
            let handshake = bridge.handshake();
            let encoding = negotiated_encoding(&handshake).to_string();
            self.unresolved_token = lsp_edits::token_type_index(&handshake, UNRESOLVED_TOKEN);
            return refuse_foreign_encoding(&encoding);
        }
        if self.server.is_some() {
            return Ok(());
        }

        // Pinning is mandatory, not best-effort. Skipping it let rust-analyzer's cargo and
        // rustc fall through the rustup proxy, which walks up to the repo-root
        // rust-toolchain.toml — non-empty `components`/`targets`, so the proxy channel-syncs
        // over the network. That is a candidate for the 600s stall at `discovering sysroot`
        // (Falcon e34fef02), and it fails silently, which is worse than failing.
        let toolchain =
            server_process::default_toolchain_name(&self.rustup_home).ok_or_else(|| {
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

        let mut command = self.spawns.command(&self.binary);
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
        self.environment =
            server_process::describe_server_environment(&self.binary, &toolchain, &toolchain_bin);
        let mut process = self
            .spawns
            .spawn(purpose::RUST_ANALYZER, &mut command)
            .map_err(|error| failure(format!("could not start rust-analyzer: {error}")))?;

        let stdin = process.take_stdin().expect("stdin was piped");
        let stdout = BufReader::new(process.take_stdout().expect("stdout was piped"));
        self.server = Some(server_process::Server {
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
        refuse_foreign_encoding(negotiated_encoding(&handshake))?;

        self.unresolved_token = lsp_edits::token_type_index(&handshake, UNRESOLVED_TOKEN);
        self.notify("initialized", json!({}))
    }

    fn request(&mut self, id: u64, method: &str, params: Value) -> Result<Value> {
        if let Some(bridge) = &self.bridge {
            let started = Instant::now();
            let outcome = bridge.request(method, params);
            // The self-spawned transport folds progress in as it reads the stream; a bridged one
            // never sees the stream, so it collects what arrived and folds it in here. Without
            // this the whole load is silent and a timeout cannot say where the server got to.
            for notification in bridge.notifications_to_fold() {
                if let Some(line) = self.chatter.absorb(&notification) {
                    (self.progress)(&line);
                }
            }
            // A request the run's own token ended is the caller having stopped, and it is reported
            // as the incomplete index it is — folded *after* the drain above, so `how_far` names
            // the furthest the load actually got rather than where it stood one request ago. Not
            // retryable, which is what keeps `request_settled` from re-asking a server on behalf
            // of somebody who has gone.
            return match outcome {
                Err(RestructureError::CallerStopped) => {
                    Err(self.incomplete_index(started.elapsed()))
                }
                other => other,
            };
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
                        _ => server_defect(format!("rust-analyzer: {error}")),
                    });
                }
                return Ok(message.get("result").cloned().unwrap_or(Value::Null));
            }
        }
    }

    /// Issue a request, re-sending while the server reports it is still catching up.
    ///
    /// The retry count is a bound on one method's *answers*, not on indexing: a server that keeps
    /// asking to be asked again is not making progress this client can wait out, and it is the one
    /// case where waiting longer is not the remedy. Exhausting it says the server would not settle
    /// — which is a different thing from the plan being wrong, and is reported as such.
    fn request_settled(&mut self, method: &str, params: Value) -> Result<Value> {
        let started = Instant::now();
        for _ in 0..CONTENT_MODIFIED_RETRIES {
            let id = self.take_id();
            match self.request(id, method, params.clone()) {
                Err(RestructureError::ServerCatchingUp) => {
                    if !self.keep_waiting(SETTLE_POLL) {
                        return Err(self.incomplete_index(started.elapsed()));
                    }
                }
                outcome => return outcome,
            }
        }
        Err(unsettled(method, started.elapsed(), self.chatter.how_far()))
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

    /// Whether the server can type a position — the cheapest proxy for "inference is ready here".
    ///
    /// Hover is answered from inference rather than from the syntax tree, so a null hover on a
    /// position that certainly has a type means the server has not typed this body yet.
    fn inference_ready_at(&mut self, uri: &str, at: Position) -> Result<bool> {
        let hover = self.request_settled(
            "textDocument/hover",
            json!({ "textDocument": { "uri": uri }, "position": lsp_position(at) }),
        )?;
        Ok(!hover.is_null())
    }

    /// Ask for a named assist, waiting while the crate graph is still loading. `probe` is where the
    /// server is asked whether inference is ready, which for a range must be a position that can
    /// carry a hover.
    ///
    /// The wait ends on one of three things and never on a clock. The assist arrives, and that is
    /// the answer. Or the server is demonstrably ready *here* and still does not offer it, which is
    /// the range being wrong rather than the server being slow — and that readiness is the very
    /// evidence the old deadline used to gather once it had expired. Or the caller stops waiting.
    fn assist(
        &mut self,
        uri: &str,
        range: Range,
        kind: RefactorKind,
        probe: Position,
    ) -> Result<Value> {
        let assist = assist_for(kind)
            .ok_or_else(|| failure(format!("no rust-analyzer assist maps to {kind:?}")))?;
        self.offered_assist(uri, range, assist, probe)
    }

    /// The code action `assist` names, once the server offers it at `range`.
    fn offered_assist(
        &mut self,
        uri: &str,
        range: Range,
        assist: Assist,
        probe: Position,
    ) -> Result<Value> {
        let wanted = assist.title;
        let target = if assist.at_caret {
            Range {
                start: range.start,
                end: range.start,
            }
        } else {
            range
        };
        let started = Instant::now();
        // #500's account of what the wait is for, said once rather than per poll: a reader needs to
        // know which assist is outstanding, not how many times it has been asked for.
        let mut reported_wait = false;
        loop {
            if !reported_wait {
                (self.progress)(&format!("waiting for assist `{wanted}`"));
                reported_wait = true;
            }
            let actions = self.request_settled(
                "textDocument/codeAction",
                json!({
                    "textDocument": { "uri": uri },
                    "range": lsp_range(target),
                    "context": context_for(assist.kinds)
                }),
            )?;

            if let Some(action) = titled(&actions, wanted) {
                return Ok(action);
            }

            // An assist that needs inference is absent for two indistinguishable reasons: the
            // range does not support it, or inference is not ready *there*. `indexed` is a
            // whole-file flag set from a hover on the file's first symbol, which on a large module
            // says nothing about a body thousands of lines further down. So ask at the range
            // itself before blaming the plan.
            let inference = if assist.needs_inference {
                Some(self.inference_ready_at(uri, probe)?)
            } else {
                None
            };
            let offered = offered_titles(&actions);
            if inference.unwrap_or(self.indexed) {
                return Err(absent_assist(wanted, &offered));
            }
            if !self.keep_waiting(INDEXING_POLL) {
                return Err(incomplete_assist_index(
                    wanted,
                    &offered,
                    inference,
                    started.elapsed(),
                    self.chatter.how_far(),
                    self.environment.clone(),
                ));
            }
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
        if op.op == RefactorKind::MoveItem {
            return item_move::findings(op, workspace);
        }
        if op.op == RefactorKind::ReparentModule {
            return module_reparent::findings(op, workspace);
        }
        let Anchor::Range { start, end, .. } = &op.anchor else {
            return Ok(Vec::new());
        };

        let text = workspace.read(op.anchor.file())?;
        let planned = Range {
            start: *start,
            end: *end,
        };
        let mut findings = Vec::new();

        if let Err(refusal) = module_text::refuse_split_attribute_paths(&text, planned) {
            findings.push(refusal.to_string());
        }
        if op.op == RefactorKind::ExtractMethod {
            findings.extend(
                refuse_early_returns(&text, planned)
                    .err()
                    .map(|e| e.to_string()),
            );
            // Not a finding: the carry is what `apply` does about it, so the plan is sound.
            for statement in imports::local_uses_to_carry(&text, planned) {
                (self.progress)(&format!(
                    "the extracted function will carry the function-local `{statement}`"
                ));
            }
        }

        if op.op == RefactorKind::ExtractModule {
            if let Some(name) = op.name.as_deref() {
                let file = op.anchor.file();
                if let Err(refusal) = module_text::refuse_module_name_taken(&text, name, planned) {
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
        self.closing_what_it_opens(|backend| backend.anchor_opening(file, items, workspace))
    }

    fn resolve(&mut self, op: &RefactorOp, workspace: &Workspace<'_>) -> Result<Resolution> {
        self.closing_what_it_opens(|backend| backend.resolve_opening(op, workspace))
    }

    /// This backend *is* the reference engine a cross-crate move surveys through — the same
    /// `documentSymbol` + `textDocument/references` implementation the move itself resolves with, so
    /// a `check --deep` rehearsal reports the blast radius an apply would act on and not a second
    /// approximation of it.
    fn module_references(&mut self) -> Option<&mut dyn ModuleReferences> {
        Some(self)
    }

    fn item_resolver(&mut self) -> Option<&mut dyn ItemResolver> {
        Some(self)
    }

    fn item_locator(&mut self) -> Option<&mut dyn ItemAtResolver> {
        Some(self)
    }
}

/// The engine half of a cross-crate move.
///
/// `move_module_to_crate` decides what to write; this decides what is out there to be written to.
/// Both halves of the answer are the server's own: `documentSymbol` for the items a module path can
/// name, and `textDocument/references` for every place outside the file that names one.
impl ModuleReferences for RustBackend {
    fn outside_references(
        &mut self,
        workspace: &Workspace<'_>,
        file: &str,
    ) -> Result<Vec<ItemReferences>> {
        self.closing_what_it_opens(|backend| backend.outside_references_opening(workspace, file))
    }
}

impl RustBackend {
    /// [`LanguageBackend::resolve`], with the documents it opens left for the caller to close.
    fn resolve_opening(
        &mut self,
        op: &RefactorOp,
        workspace: &Workspace<'_>,
    ) -> Result<Resolution> {
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
            module_text::refuse_split_attribute_paths(&original, planned)?;
            if op.op == RefactorKind::ExtractMethod {
                refuse_early_returns(&original, planned)?;
            }
            if op.op == RefactorKind::ExtractModule {
                if let Some(name) = op.name.as_deref() {
                    module_text::refuse_module_name_taken(&original, name, planned)?;
                }
            }
        }

        // rust-analyzer has no cross-crate move assist, so this operation is authored rather than
        // delegated — every caller it re-points still comes from the server's own reference set,
        // which is what this backend supplies it. It opens the module for itself, so the document
        // is deliberately not opened here first.
        if op.op == RefactorKind::MoveModuleToCrate {
            (self.progress)("cross-crate move: surveying callers and building edits");
            return Ok(Resolution::of(crate_move::resolve(self, workspace, op)?));
        }

        // Moves items between modules of one crate: authored here and informed by the server, like
        // the cross-crate moves, and opens the document for itself.
        if op.op == RefactorKind::MoveItem {
            return self.move_items(op, workspace);
        }

        // Moves a module and its directory under another parent of the same crate, authored here
        // the same way and opening the parent's document for itself.
        if op.op == RefactorKind::ReparentModule {
            return self.reparent_module(op, workspace);
        }

        // The same operation over a set, and one edit rather than one per member: a
        // mutually-referencing set has no order in which the tree compiles between moves, so every
        // member's callers are surveyed against where the whole set is going.
        if op.op == RefactorKind::MoveClusterToCrate {
            let cluster = crate_move::named_by(workspace, op)?;
            (self.progress)(&format!(
                "cross-crate move of {} modules: surveying callers and building edits",
                cluster.members.len()
            ));
            return Ok(Resolution::of(crate_move::resolve_cluster(
                self, workspace, &cluster,
            )?));
        }

        // The same move with the reference survey taken out of it: cargo builds each `tests/*.rs`
        // as its own crate root, so nothing in the workspace can name a test binary and there is
        // no caller to ask the server about. What is left is the file, its own `use` header, and
        // the manifest that has to compile it — all of which this backend reads for itself.
        if op.op == RefactorKind::MoveTestBinaryToCrate {
            let moving = crate_move::read_test_binary_move(workspace, op)?;
            (self.progress)(&format!(
                "moving test binary `{}` to {}",
                moving.name, moving.destination.package
            ));
            return Ok(Resolution::of(crate_move::resolve_test_binary_move(
                workspace, &moving,
            )?));
        }

        // The signature and call-site operations are edits this backend writes to the one
        // declaration or call the anchor names, so they are answered by reading the text — before a
        // server exists, like the refusals above.
        if let Some(edits) = Self::rewrite_signature(op, &original)? {
            return Ok(Resolution::of(WorkspaceEdit {
                changes: vec![FileEdit::Change {
                    path: relative,
                    edits,
                }],
            }));
        }

        // `change_return_type` with a `variant` is the one signature operation rust-analyzer has an
        // assist for: it rewrites the declaration and the function's own returns, in its own file.
        if op.op == RefactorKind::ChangeReturnType {
            self.start(workspace.root)?;
            self.did_open(&uri, &original)?;
            self.ensure_indexed(&uri)?;
            let edits = self.wrap_or_unwrap_return_type(&uri, &original, op)?;
            return Ok(Resolution::of(WorkspaceEdit {
                changes: vec![FileEdit::Change {
                    path: relative,
                    edits,
                }],
            }));
        }

        self.start(workspace.root)?;
        self.did_open(&uri, &original)?;
        self.ensure_indexed(&uri)?;

        // An assist that reaches past the anchor's own file produces its edits directly rather
        // than through the single-document path the in-place assists share.
        if assist_for(op.op).is_some_and(|assist| assist.multi_file) {
            return Ok(Resolution::of(self.multi_file_assist(&uri, workspace, op)?));
        }

        // A rename reaches every document the server names, so it produces its edits directly
        // rather than through the single-document path the in-place assists share.
        if op.op == RefactorKind::RenameSymbol {
            return Ok(Resolution::of(self.rename_symbol(&uri, workspace, op)?));
        }

        let (final_text, report, notes) = self.assisted_edit(&uri, &original, op)?;

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

    /// [`LanguageBackend::anchor_for`], with the documents it opens left for the caller to close.
    fn anchor_opening(
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

        (self.progress)("building anchor from module outline");
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

    /// [`ModuleReferences::outside_references`], with the documents it opens left for the caller
    /// to close.
    fn outside_references_opening(
        &mut self,
        workspace: &Workspace<'_>,
        file: &str,
    ) -> Result<Vec<ItemReferences>> {
        let text = workspace.read(file)?;
        let uri = uri_of(&workspace.root.join(file));

        self.start(workspace.root)?;
        self.did_open(&uri, &text)?;
        self.ensure_indexed(&uri)?;

        let symbols = self.request_settled(
            "textDocument/documentSymbol",
            json!({ "textDocument": { "uri": uri } }),
        )?;

        let mut found = Vec::new();
        for item in path_reached_within(&symbols, whole_of(&text)) {
            // An item nested in an inline module is reached through that module's own name, which
            // is itself one of these — so re-pointing the outer name carries the inner path with
            // it, and naming the inner one flat would write a facade line that resolves to nothing.
            if !item.within.is_empty() {
                continue;
            }

            found.push(ItemReferences {
                referenced_at: self.references_outside(&uri, &item.position, workspace)?,
                item: item.name,
            });
        }

        Ok(found)
    }
}

impl RustBackend {
    /// Every place outside `uri` that names the item at `position`.
    ///
    /// Columns come back as byte offsets, per the encoding this client negotiates, and are
    /// converted to the character columns [`crate::edit::Position`] is read in — the one place the
    /// two counts have to be reconciled, because a caller is addressed in a file this backend never
    /// opened.
    fn references_outside(
        &mut self,
        uri: &str,
        position: &Value,
        workspace: &Workspace<'_>,
    ) -> Result<Vec<Reference>> {
        let references = self.references_at(uri, position)?;

        let mut sites = Vec::new();
        for reference in references.as_array().into_iter().flatten() {
            let referrer = reference
                .get("uri")
                .and_then(Value::as_str)
                .ok_or_else(|| server_defect("a reference carries no uri"))?;
            if referrer == uri {
                continue;
            }

            let path = relative_to(referrer, workspace.root)?;
            let text = workspace.read(&path)?;
            let at = character_column(&text, LspPoint::read(reference.pointer("/range/start"))?);
            sites.push(Reference { path, at });
        }

        Ok(sites)
    }

    /// Run an assist that introduces a new symbol, then give that symbol its real name.
    fn assisted_edit(
        &mut self,
        uri: &str,
        original: &str,
        op: &RefactorOp,
    ) -> Result<(String, Vec<VisibilityChange>, Vec<String>)> {
        (self.progress)(&format!("assist: {:?} in this file", op.op));
        let range = self.anchor_range(uri, op)?;
        // A place selected under a borrow is bound as the borrow: the assist would otherwise bind
        // it by value, which moves a non-`Copy` field out of `&self`. Where the borrow cannot be
        // included, the extraction is refused instead.
        let range = if op.op == RefactorKind::ExtractVariable {
            let widened = selection::widened_to_its_borrow(original, range);
            selection::refuse_by_value_hoist(original, widened)?;
            widened
        } else {
            range
        };
        let relocates = assist_for(op.op).is_some_and(|assist| assist.relocates_items);
        let reexport = op.reexport.unwrap_or(Reexport::None);

        // An assist whose output embeds inferred types has to wait for inference, not merely for the
        // assist to be offered — those are different readiness signals, and the second arrives first.
        // A symbol anchor already waits inside `anchor_range`; a range anchor has nothing to wait on
        // there, because there is no symbol to resolve.
        if assist_for(op.op).is_some_and(|assist| assist.needs_inference) {
            let probe = selection::hover_bearing_position(original, range);
            self.wait_until_resolved_within_bound(uri, &lsp_edits::lsp_position(probe))?;
        }

        let moved = if relocates {
            self.survey_moved_items(uri, original, range)?
        } else {
            Vec::new()
        };

        // A facade leaves the old path resolving through the parent, so a reference elsewhere is no
        // longer stranded and there is nothing here to refuse.
        if relocates && reexport == Reexport::None {
            seam_survey::refuse_stranded(&moved)?;
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
        let (extracted, introduced) =
            introduced_by(placeholder, original, extracted, &impl_members, &moved)?;

        let named = self.rename_placeholder(uri, &extracted, &introduced, &name)?;
        // A symbol already bearing the plan's name was not renamed, so every site of it is meant.
        if introduced.name != name {
            placeholder_checks::refuse_residual_placeholder(original, &named, &introduced.name)?;
        }
        if placeholder.declares_a_signature() {
            placeholder_checks::refuse_inferred_placeholder(
                &named,
                &format!("{} {name}", placeholder.keyword),
            )?;
        }

        if !relocates {
            // The new function lands outside the one it came from, where that function's own `use`
            // items are not in scope.
            let named = if op.op == RefactorKind::ExtractMethod {
                imports::carry_function_local_uses(original, &named, range, &name)
            } else {
                named
            };
            return Ok((named, Vec::new(), Vec::new()));
        }

        // Before the import passes, because a seam the assist only half-took is not one those
        // passes can repair — and refusing here costs the operator two LSP round trips rather than
        // the whole import restoration on a module that is not the one they asked for.
        visibility::refuse_partial_relocation(&named, &name, &moved)?;

        // Versions 1 and 2 belong to the open and to the rename above; both import phases send
        // more, so the counter runs across them rather than restarting.
        let pruned = self.prune_assist_imports(uri, &named, &name)?;
        let imported = self.restore_imports(uri, original, &pruned, &name, &moved, reexport)?;
        let imported = prelude_shadow::carry_shadowed_imports(original, &imported, &name)?;
        let (preserved, mut report) = visibility::restore_visibility(&imported, &name, &moved)?;
        let (preserved, rerooted) = inline_paths::rerooted_module(&preserved, &name)?;

        // The widenings the pass above cannot see, because the survey feeding it stops above an
        // `impl`. Read off the text the assist actually produced, so a member it left alone is not
        // reported as though it had moved.
        let relocated: Vec<String> = preserved.split('\n').map(str::to_string).collect();
        report.extend(facade::impl_widenings(
            &relocated,
            &module_text::module_bounds(&relocated, &name)?,
            &impl_members,
        ));
        facade::refuse_mangled_rewrite(&preserved, &name, &moved)?;
        let facade = facade::facade_lines(&name, &moved, reexport)?;
        let notes = facade::empty_facade_note(&name, &facade, reexport)
            .into_iter()
            .chain(inline_paths::note(rerooted))
            .collect();

        Ok((
            server_process::without_hollow_imports(&module_text::with_facade(
                &preserved, &name, &facade,
            )?),
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
    ) -> Result<Vec<seam_survey::MovedItem>> {
        let symbols = self.request_settled(
            "textDocument/documentSymbol",
            json!({ "textDocument": { "uri": uri } }),
        )?;

        let mut items = Vec::new();

        for found in path_reached_within(&symbols, range) {
            let reach = self.reach_of(uri, &found.position, range)?;
            items.push(seam_survey::MovedItem {
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

    /// The document's outline, once the server is able to give one.
    ///
    /// Until the server has loaded the workspace it may answer `documentSymbol` for an open document
    /// with no symbols at all, which reads exactly like a file that defines nothing. That is how
    /// `restructure anchors` came to refuse every item of every file: it took the first answer.
    /// (`readiness.rs` notes the server otherwise answers this request from the syntax tree, so
    /// *whether* a loading server answers early-empty is an unreproduced hypothesis here.)
    ///
    /// What *is* reproduced is the opposite hazard, which the original wait had: it looped while the
    /// outline was empty and `indexed` unset, and nothing on the anchors path sets `indexed`, so a
    /// file that genuinely defines nothing never returned — against a live rust-analyzer it ran
    /// until the caller's token fired (180s in the acceptance harness).
    ///
    /// An outline with items in it is the server's real answer whenever it arrives. An empty one is
    /// believed only once the graph has been *observed* loaded — [`Self::outline_is_the_servers_answer`]
    /// — so a file that genuinely defines nothing (a `mod.rs` of `use` lines, a comments-only file)
    /// is answered as soon as the server reports itself quiescent, and a degraded index refuses
    /// rather than vouching for the emptiness. The wait is otherwise ended only by the caller's
    /// cancellation, as for every other wait here, and then names where the index got to.
    fn settled_outline(&mut self, uri: &str) -> Result<Value> {
        let started = Instant::now();
        loop {
            let symbols = self.request_settled(
                "textDocument/documentSymbol",
                json!({ "textDocument": { "uri": uri } }),
            )?;
            if self.outline_is_the_servers_answer(&symbols) {
                self.refuse_degraded_index()?;
                return Ok(symbols);
            }
            if !self.keep_waiting(INDEXING_POLL) {
                return Err(self.incomplete_index(started.elapsed()));
            }
        }
    }

    /// Whether `symbols` is an answer about the file rather than the silence of a server still
    /// loading: it has items, or the server has been seen to finish loading.
    fn outline_is_the_servers_answer(&self, symbols: &Value) -> bool {
        !outline_is_empty(symbols) || self.indexed || self.chatter.quiescent()
    }

    /// The file's module-level items, in the order they appear.
    fn module_outline(&mut self, uri: &str) -> Result<Vec<OutlineItem>> {
        let symbols = self.settled_outline(uri)?;

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
    ) -> Result<Vec<seam_survey::MovedItem>> {
        let symbols = self.request_settled(
            "textDocument/documentSymbol",
            json!({ "textDocument": { "uri": uri } }),
        )?;

        let cut = seam_survey::impls_cut_through(&symbols, range);
        let relocated = seam_survey::items_relocated_within(&symbols, range);
        (self.trace)(&seam_trace(range, &cut, &relocated));
        let mut items = Vec::new();

        for found in relocated {
            let Some(holder) = found.within.last() else {
                continue;
            };
            if placeholder_checks::declares(holder) != Some(placeholder_checks::Block::Impl) {
                continue;
            }

            // References are only asked about where the answer can change the outcome. A member of an
            // `impl` that moves whole is reached through its type and cannot be stranded, so paying a
            // resolution wait per member would buy nothing — and `survey_moved_items` paying one per
            // item is already the expensive part of a seam.
            let reach = if cut.contains(holder) {
                self.reach_of(uri, &found.position, range)?
            } else {
                seam_survey::Reach::default()
            };

            items.push(seam_survey::MovedItem {
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

    /// The server's `textDocument/references` for the item at `position`, once it can be believed.
    ///
    /// References answer empty rather than pending while the crate graph is still loading, so an
    /// empty answer is only worth believing once the position resolves at all — or once the server
    /// has said it never will, because a `#[cfg]` has switched the item off. That item is still
    /// asked about, so its empty answer is the server's own. The survey is of references under the
    /// cfg the server evaluated, for every item alike, so this says so on the progress line rather
    /// than presenting an unseen caller as none.
    fn references_at(&mut self, uri: &str, position: &Value) -> Result<Value> {
        if let readiness::Answerable::Inactive(said) = self.wait_until_answerable(uri, position)? {
            (self.progress)(&format!(
                "{} is inactive to rust-analyzer (\"{said}\"): only code under the cfg it \
                 evaluated is surveyed for references to it",
                readiness::located(uri, position)?
            ));
        }

        self.request_settled(
            "textDocument/references",
            json!({
                "textDocument": { "uri": uri },
                "position": position,
                "context": { "includeDeclaration": false }
            }),
        )
    }

    /// Where the references to one item sit, relative to the range about to be relocated.
    fn reach_of(
        &mut self,
        uri: &str,
        position: &Value,
        range: Range,
    ) -> Result<seam_survey::Reach> {
        let references = self.references_at(uri, position)?;

        let mut reach = seam_survey::Reach::default();

        for reference in references.as_array().into_iter().flatten() {
            let referrer = reference
                .get("uri")
                .and_then(Value::as_str)
                .ok_or_else(|| server_defect("a reference carries no uri"))?;

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
        let block = module_text::module_bounds(&source, module)?;

        Ok(import_text::without_dead_imports(&source, &block, &unresolved).join("\n"))
    }

    /// Every identifier in the open document that the server cannot resolve, in source order.
    fn unresolved_names(
        &mut self,
        uri: &str,
        text: &str,
    ) -> Result<Vec<import_text::UnresolvedName>> {
        let wanted = self.unresolved_token.ok_or_else(|| {
            server_defect(format!(
                "rust-analyzer's semantic token legend has no `{UNRESOLVED_TOKEN}`, so the names an \
                 extraction loses cannot be found"
            ))
        })?;

        let tokens = self.request_settled(
            "textDocument/semanticTokens/full",
            json!({ "textDocument": { "uri": uri } }),
        )?;

        Ok(lsp_edits::unresolved_in(&tokens, wanted, text))
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
        let start = lsp_edits::lsp_range(Range {
            start: named,
            end: named,
        })["start"]
            .clone();
        self.wait_until_resolved(produced.uri, &start)?;

        let action = self.assist(
            produced.uri,
            caret,
            RefactorKind::ExtractModuleToFile,
            caret.start,
        )?;
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
                parent = lsp_edits::apply_lsp_edit(&parent, edits_in(&change)?);
                continue;
            }
            changes.extend(convert_change(&change, workspace, &mut created)?);
        }

        changes.push(FileEdit::Change {
            path: produced.relative.to_string(),
            edits: seam_survey::minimal_edits(produced.original, &parent),
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
        (self.progress)(&format!("assist: {:?} (multi-file)", op.op));
        let mut range = self.anchor_range(uri, op)?;
        match op.op {
            RefactorKind::RemoveUnusedParam => {
                range = signature::parameter_caret(workspace, op, range)?
            }
            RefactorKind::ConvertTupleReturnToStruct => {
                range = signature::return_type_caret(workspace, op, range)?
            }
            _ => {}
        }

        let action = self
            .assist(uri, range, op.op, range.start)
            .map_err(|refusal| signature::refuse_used_parameter(op, refusal))?;
        let resolved = self.request_settled("codeAction/resolve", action)?;
        let workspace_edit = resolved.get("edit").unwrap_or(&resolved);

        if op.op == RefactorKind::ConvertTupleReturnToStruct {
            return self.name_converted_struct(uri, workspace, op, workspace_edit);
        }

        let mut changes = Vec::new();
        let mut created: Vec<String> = Vec::new();

        for change in document_changes(workspace_edit) {
            changes.extend(convert_change(&change, workspace, &mut created)?);
        }

        if changes.is_empty() {
            return Err(server_defect(format!(
                "rust-analyzer returned no edits for {:?}",
                op.op
            )));
        }
        Ok(WorkspaceEdit { changes })
    }

    /// The text of `original` after the signature or call-site operation `op`, or `None` when `op`
    /// is not one of those that is a text edit.
    fn rewrite_signature(op: &RefactorOp, original: &str) -> Result<Option<Vec<TextEdit>>> {
        let rewrites_a_declaration = matches!(
            op.op,
            RefactorKind::ChangeParamType | RefactorKind::AddParam | RefactorKind::ReorderParams
        ) || (op.op == RefactorKind::ChangeReturnType
            && op.type_.is_some());
        if !rewrites_a_declaration && !op.op.edits_a_call_site() {
            return Ok(None);
        }

        let Anchor::Range { start, end, .. } = &op.anchor else {
            return Err(unlowered_item_anchor(&op.anchor, &format!("{:?}", op.op)));
        };
        if rewrites_a_declaration {
            signature_rewrites::rewrite_declaration(original, op, *start).map(Some)
        } else {
            let range = Range {
                start: *start,
                end: *end,
            };
            signature_rewrites::rewrite_call(original, op, range).map(Some)
        }
    }

    /// The edits of rust-analyzer's `wrap_result`, `wrap_option` or `unwrap` assist, asked for with
    /// the caret on the return type of the function the anchor names.
    ///
    /// The server's own edits are reported as they came, not as a line diff of the document they
    /// produce: a later operation of the same plan anchors on this function's name, on the line the
    /// assist rewrites.
    fn wrap_or_unwrap_return_type(
        &mut self,
        uri: &str,
        original: &str,
        op: &RefactorOp,
    ) -> Result<Vec<TextEdit>> {
        let variant = op
            .variant
            .as_deref()
            .ok_or_else(|| failure("`change_return_type` needs `type` or `variant`"))?;
        let name = self.anchor_range(uri, op)?.start;
        let returned = signature_rewrites::returned_type(original, name);
        let assist = return_type::return_type_assist(variant, returned.as_deref())?;
        let caret = signature::return_type_position(original, name).ok_or_else(|| {
            seam_refusal("the function the anchor names declares no return type to rewrite")
        })?;
        let range = Range {
            start: caret,
            end: caret,
        };

        (self.progress)(&format!("assist: {variant} the return type"));
        let action = self.offered_assist(uri, range, assist, caret)?;
        let resolved = self.request_settled("codeAction/resolve", action)?;
        Ok(edits_for(&resolved, uri)?
            .into_iter()
            .map(|edit| TextEdit {
                range: Range {
                    start: Position {
                        line: edit.start.line as u32 + 1,
                        col: edit.start.character as u32 + 1,
                    },
                    end: Position {
                        line: edit.end.line as u32 + 1,
                        col: edit.end.character as u32 + 1,
                    },
                },
                new_text: edit.new_text,
            })
            .collect())
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
            Anchor::Item { .. } | Anchor::Items { .. } => {
                Err(unlowered_item_anchor(&op.anchor, &format!("{:?}", op.op)))
            }
        }
    }

    /// Rename the symbol an anchor points at, using the server's own rename.
    ///
    /// Both anchor kinds work: a range names a position directly, and a symbol is resolved through
    /// `workspace/symbol` so a plan need not carry different anchors per language.
    ///
    /// Every document the server names is edited, not only the anchor's own. rust-analyzer computes
    /// the cross-file edits; keeping one of them is what left a caller in another file naming a
    /// symbol that no longer existed, and the anchor's file looked right the whole time.
    fn rename_symbol(
        &mut self,
        uri: &str,
        workspace: &Workspace<'_>,
        op: &RefactorOp,
    ) -> Result<WorkspaceEdit> {
        let name = op
            .name
            .clone()
            .ok_or_else(|| failure("rename_symbol needs a name"))?;
        (self.progress)(&format!(
            "rename: `{name}` — collecting edits from rust-analyzer"
        ));
        let position = match &op.anchor {
            Anchor::Range { start, .. } => {
                json!({ "line": start.line - 1, "character": start.col - 1 })
            }
            Anchor::Symbol { path, .. } => self.locate_symbol(uri, path)?,
            Anchor::Item { .. } | Anchor::Items { .. } => {
                return Err(unlowered_item_anchor(&op.anchor, &format!("{:?}", op.op)))
            }
        };

        self.wait_until_resolved(uri, &position)?;

        let renamed = self.request_settled(
            "textDocument/rename",
            json!({ "textDocument": { "uri": uri }, "position": position, "newName": name }),
        )?;

        let mut changes = Vec::new();
        for (document, edits) in lsp_edits::workspace_edits_for(&renamed)? {
            let path = relative_to(&document, workspace.root)?;
            // Read through the overlay, as every other multi-document path does, so a caller an
            // earlier operation in the same plan already edited is renamed against that text.
            let original = workspace.read(&path)?;
            let updated = lsp_edits::apply_lsp_edit(&original, edits);
            changes.push(FileEdit::Change {
                path,
                edits: seam_survey::minimal_edits(&original, &updated),
            });
        }

        Ok(WorkspaceEdit { changes })
    }

    /// Where a named symbol is declared in the open document.
    ///
    /// Asked of the document rather than the workspace, so the answer does not depend on how a URI
    /// is spelled. Polled for the same reason the assists are: until the crate graph is loaded the
    /// server answers with no symbols, and a rename against an unresolved position is refused.
    ///
    /// An outline the server did answer is its real answer, so a name missing from one is missing
    /// from the file — that is a mistake in the request, and waiting cannot fix it.
    fn locate_symbol(&mut self, uri: &str, name: &str) -> Result<Value> {
        let started = Instant::now();
        loop {
            let symbols = self.request_settled(
                "textDocument/documentSymbol",
                json!({ "textDocument": { "uri": uri } }),
            )?;

            if let Some(position) = find_symbol(&symbols, name) {
                return Ok(position);
            }
            if self.indexed || !outline_is_empty(&symbols) {
                return Err(failure(format!("`{name}` is not declared in this file")));
            }
            if !self.keep_waiting(INDEXING_POLL) {
                return Err(self.incomplete_index(started.elapsed()));
            }
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
        let action = self.assist(
            uri,
            range,
            kind,
            selection::hover_bearing_position(original, range),
        )?;
        let resolved = self.request_settled("codeAction/resolve", action)?;
        Ok(lsp_edits::apply_lsp_edit(
            original,
            lsp_edits::edits_for(&resolved, uri)?,
        ))
    }

    /// Give the symbol the assist introduced its real name.
    ///
    /// The rename is asked of the server rather than performed here, so no identifier in the
    /// result — and no reference to it anywhere else — is written by this backend. A symbol the
    /// server already named as the plan asks is left as it is.
    fn rename_placeholder(
        &mut self,
        uri: &str,
        extracted: &str,
        introduced: &Introduced,
        name: &str,
    ) -> Result<String> {
        self.did_change(uri, extracted)?;
        if introduced.name == name {
            return Ok(extracted.to_string());
        }
        let definition = introduced.offset;

        // An assist that introduces a top-level item leaves the server rebuilding the module tree,
        // and a rename that arrives first is refused outright rather than deferred.
        let position = lsp_edits::position_at(extracted, definition);
        self.wait_until_resolved(uri, &position)?;

        let renamed = self.request_settled(
            "textDocument/rename",
            json!({
                "textDocument": { "uri": uri },
                "position": position,
                "newName": name
            }),
        )?;
        Ok(lsp_edits::apply_lsp_edit(
            extracted,
            lsp_edits::edits_for(&renamed, uri)?,
        ))
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

    Err(seam_refusal(format!(
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
/// Refuse a handshake that settled on any position unit but the one this client counts.
///
/// Shared by both transports on purpose: the self-spawned path asks for [`BYTE_ENCODING`] in its
/// own capabilities and would be astonished not to get it, while a bridged client was
/// initialized by someone else and may never have asked at all.
fn refuse_foreign_encoding(encoding: &str) -> Result<()> {
    if encoding == BYTE_ENCODING {
        return Ok(());
    }
    Err(server_defect(format!(
        "rust-analyzer settled on `{encoding}` positions, and this client counts \
         `{BYTE_ENCODING}` — every column it converted would be wrong on any line carrying a \
         character outside the BMP. Refusing rather than resolving anchors against the wrong \
         unit."
    )))
}

/// Every code-action title the server offered, in the order it offered them.
fn offered_titles(actions: &Value) -> Vec<String> {
    actions
        .as_array()
        .map(|actions| {
            actions
                .iter()
                .filter_map(|action| action.get("title").and_then(Value::as_str))
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// Whether a `documentSymbol` answer holds no items — which is how the server answers while it is
/// still loading the crate graph, and therefore not yet an answer about the file.
fn outline_is_empty(symbols: &Value) -> bool {
    symbols
        .as_array()
        .map(|items| items.is_empty())
        .unwrap_or(true)
}

/// The failure to report when the server is ready to answer for a range and offers no such assist.
///
/// This is a seam refusal: the reader should look at the range they asked for. What the server
/// *did* offer is named, because an assist that is absent under one title and present under
/// another is otherwise indistinguishable from a range that supports no refactoring at all.
fn absent_assist(wanted: &str, offered: &[String]) -> RestructureError {
    let offered = if offered.is_empty() {
        "it offered none".to_string()
    } else {
        format!("it offered: {}", offered.join(", "))
    };
    seam_refusal(format!(
        "rust-analyzer offers no \"{wanted}\" assist for the given range ({offered})"
    ))
}

/// The failure to report when a wait for an assist ended before the server could offer it.
///
/// A range the server answered `codeAction` for but could not *type* is named as such: the assist
/// needing inference was never going to be in that list, and reporting it as an absent assist
/// sends the reader to rewrite an anchor that was correct.
fn incomplete_assist_index(
    wanted: &str,
    offered: &[String],
    inference_ready: Option<bool>,
    waited: Duration,
    last: String,
    environment: String,
) -> RestructureError {
    if inference_ready == Some(false) {
        return RestructureError::IndexingIncomplete {
            seconds: waited.as_secs(),
            last: format!(
                "{last} — the server answered `codeAction` but could not type the range, so \
                 `{wanted}` (which needs type inference) was never offered; it offered only {}",
                if offered.is_empty() {
                    "nothing".to_string()
                } else {
                    offered.join(", ")
                }
            ),
            environment,
        };
    }
    RestructureError::IndexingIncomplete {
        seconds: waited.as_secs(),
        last,
        environment,
    }
}

/// The failure to report when the server stayed unable to answer one method.
///
/// Kept apart from a malformed plan on purpose: the plan was not wrong, the indexer never settled,
/// and a caller acts on the difference — one is fixed by editing the plan and the other by looking
/// at the server. Where the index got to travels with it, for the same reason a cancelled wait
/// carries it.
fn unsettled(method: &str, waited: Duration, last: String) -> RestructureError {
    RestructureError::ServerNotSettled {
        method: method.to_string(),
        seconds: waited.as_secs(),
        last,
    }
}

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

    Err(seam_refusal(format!(
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
        let opens = start == 0 || !placeholder_checks::is_identifier_byte(bytes[start - 1]);
        let closes = end >= bytes.len() || !placeholder_checks::is_identifier_byte(bytes[end]);
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
        let value = value.ok_or_else(|| server_defect("edit is missing a position"))?;
        let field = |name: &str| {
            value
                .get(name)
                .and_then(Value::as_u64)
                .ok_or_else(|| server_defect(format!("position is missing `{name}`")))
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
        .ok_or_else(|| server_defect(format!("`{uri}` is not a file uri")))
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

/// The range covering a whole document, for a survey that asks about all of it.
fn whole_of(text: &str) -> Range {
    Range {
        start: Position { line: 1, col: 1 },
        end: Position {
            line: text.lines().count() as u32 + 1,
            col: 1,
        },
    }
}

/// An LSP position as a one-based line and *character* column.
fn character_column(text: &str, point: LspPoint) -> Position {
    let line_start = lsp_edits::offset_of(
        text,
        LspPoint {
            line: point.line,
            character: 0,
        },
    );
    let line = &text[line_start..];
    // Count characters that start strictly before the byte offset. The obvious
    // `line[..point.character].chars().count()` is only defined when the offset lands on a UTF-8
    // boundary, and falling back to the byte offset there would report a byte count as a character
    // column — a plausible-looking wrong answer for exactly the non-ASCII line where it matters.
    // This form is in character units for every offset, boundary or not.
    let column = line
        .char_indices()
        .take_while(|(offset, _)| *offset < point.character)
        .count();

    Position {
        line: point.line as u32 + 1,
        col: column as u32 + 1,
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
        .ok_or_else(|| server_defect("a document change carries no uri"))?;
    relative_to(uri, root)
}

/// The workspace-relative path a `file://` uri names.
///
/// A uri outside the root is refused rather than skipped: an edit this executor cannot address is
/// one the operation was counting on, and dropping it is how a rename half-lands.
fn relative_to(uri: &str, root: &Path) -> Result<String> {
    let path = uri
        .strip_prefix("file://")
        .ok_or_else(|| server_defect(format!("`{uri}` is not a file uri")))?;

    Path::new(path)
        .strip_prefix(root)
        .map(|relative| relative.display().to_string())
        .map_err(|_| server_defect(format!("`{path}` lies outside the workspace")))
}

/// The text edits one `documentChanges` entry carries.
fn edits_in(change: &Value) -> Result<Vec<LspEdit>> {
    change
        .get("edits")
        .and_then(Value::as_array)
        .ok_or_else(|| server_defect("a document change carries no edits"))?
        .iter()
        .map(lsp_edits::read_edit)
        .collect()
}

mod import_text;
#[cfg(test)]
use import_text::choose_import;
#[cfg(test)]
use import_text::imported_paths;
#[cfg(test)]
use import_text::parent_module;
#[cfg(test)]
use import_text::without_dead_imports;
use import_text::{
    already_bound, group_members, import_order, occurrences_of, reached_through_qualifier,
    use_tree, UnresolvedName,
};

mod lsp_edits;
#[cfg(test)]
use lsp_edits::position_at;
#[cfg(test)]
use lsp_edits::token_type_index;
#[cfg(test)]
use lsp_edits::unresolved_in;
use lsp_edits::{apply_lsp_edit, edits_for, lsp_position, lsp_range};

mod placeholder_checks;
#[cfg(test)]
use placeholder_checks::carries_placeholder_type;
#[cfg(test)]
use placeholder_checks::placeholder_sites;
#[cfg(test)]
use placeholder_checks::refuse_inferred_placeholder;
#[cfg(test)]
use placeholder_checks::refuse_residual_placeholder;
use placeholder_checks::{declares, Block};

mod seam_survey;
#[cfg(test)]
use seam_survey::refuse_stranded;
use seam_survey::{minimal_edits, MovedItem};

mod facade;
#[cfg(test)]
use facade::empty_facade_note;
#[cfg(test)]
use facade::facade_lines;
use facade::facade_will_bind;
#[cfg(test)]
use facade::impl_widenings;
#[cfg(test)]
use facade::refuse_mangled_rewrite;

mod module_text;
#[cfg(test)]
use module_text::aliased_bindings;
#[cfg(test)]
use module_text::attribute_path_names;
#[cfg(test)]
use module_text::refuse_module_name_taken;
#[cfg(test)]
use module_text::refuse_split_attribute_paths;
#[cfg(test)]
use module_text::with_facade;
use module_text::{alias_target, module_bounds, parent_binding, with_module_import, ModuleBlock};

mod visibility;
#[cfg(test)]
use visibility::refuse_partial_relocation;
#[cfg(test)]
use visibility::restore_visibility;
use visibility::{declares_at_widened_visibility, declares_item, reaches_through_module, WIDENED};

mod line_diff;

fn uri_of(path: &Path) -> String {
    format!("file://{}", path.display())
}

/// A refusal the author fixes by editing their plan: a name the operation needs and did not carry,
/// an item the file does not define, a kind no assist implements.
///
/// The transport failures stay here too — a server that would not start, one that is not running,
/// a connection that closed. They are not a plan defect either, but `Io` and `ServerCatchingUp`
/// already sit beside this variant for the cases a caller routes differently, and a fourth class
/// nobody acts on differently would be ceremony.
fn failure(reason: impl Into<String>) -> RestructureError {
    RestructureError::MalformedPlan(reason.into())
}

/// A refusal the author fixes by cutting the seam elsewhere, or by changing the code.
///
/// Stranded references, an `impl` cut in half, a module name already taken, a name the file's own
/// imports cannot disambiguate. The plan is well formed and says exactly what it meant; the code
/// will not permit it. Every one of these messages already ends with its own remedy, and for as
/// long as they all arrived as [`failure`] the sentence in front of that remedy told the author to
/// go and edit a plan that was correct.
fn seam_refusal(reason: impl Into<String>) -> RestructureError {
    RestructureError::SeamRefused(reason.into())
}

/// A refusal nothing in the plan or the tree can fix: rust-analyzer answered, and the answer could
/// not be used.
///
/// An extraction produced before the types were inferred, a rewrite that came back with an
/// identifier written over itself, a response carrying no edits at all. The remedy these messages
/// give is a retry against a warm server or a look at the server — neither of which is something an
/// author does to a plan.
fn server_defect(reason: impl Into<String>) -> RestructureError {
    RestructureError::ServerDefect(reason.into())
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

        let found = lsp_edits::unresolved_in(&tokens, 3, text);

        assert_eq!(found.len(), 1);
        assert_eq!(found[0].text, "HashSet");
        assert_eq!(found[0].position, json!({ "line": 1, "character": 8 }));
    }

    #[test]
    fn keeps_only_the_tokens_of_the_requested_type() {
        let text = "let a = BTreeMap::new();\n";
        let tokens = json!({ "data": [0, 4, 1, 9, 0, 0, 4, 8, 3, 0] });

        let found = lsp_edits::unresolved_in(&tokens, 3, text);

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
        let paths = import_text::imported_paths("use lopdf::{Document, Object, dictionary};\n");

        assert_eq!(
            paths,
            ["lopdf::Document", "lopdf::Object", "lopdf::dictionary"]
        );
    }

    #[test]
    fn expands_a_group_nested_inside_another() {
        let paths = import_text::imported_paths(
            "use std::{collections::{HashMap, HashSet}, sync::Mutex};\n",
        );

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
        let paths = import_text::imported_paths("use crate::core::{\n    Alpha,\n    Beta,\n};\n");

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
        let paths = import_text::imported_paths(
            "/// Callers write `use lopdf::Object;` themselves.\nlet a = 1;\n",
        );

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

        let chosen = import_text::choose_import("use lopdf::{Document, Object};\n", &offered);

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
        let edits = seam_survey::minimal_edits("a\nb\nc\n", "a\nB\nc\n");

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

        let edits = seam_survey::minimal_edits(before, after);

        // Lines 2..5 are shared, so no hunk may cover them.
        assert!(edits
            .iter()
            .all(|edit| edit.range.end.line <= 2 || edit.range.start.line >= 6));
    }

    /// `line_count` counts newlines while the replaced span is `end.line - start.line`, so a
    /// replacement missing its terminator shifts every later anchor by one.
    #[test]
    fn terminates_every_replacement_with_a_newline() {
        let edits = seam_survey::minimal_edits("a\nb\nc\n", "a\nB\nB2\nc\n");

        assert!(edits
            .iter()
            .all(|edit| edit.new_text.is_empty() || edit.new_text.ends_with('\n')));
    }

    #[test]
    fn reports_a_pure_deletion_as_an_empty_replacement() {
        let edits = seam_survey::minimal_edits("a\nb\nc\n", "a\nc\n");

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
        let edits = seam_survey::minimal_edits("", "mod counting;\n");

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

        let edits = seam_survey::minimal_edits(before, after);

        for pair in edits.windows(2) {
            assert!(pair[0].range.end.line <= pair[1].range.start.line);
        }
    }

    #[test]
    fn rebuilds_the_later_text_from_the_hunks_it_reported() {
        let before = "alpha\nbeta\ngamma\ndelta\nepsilon\n";
        let after = "alpha\nBETA\ngamma\ndelta\nEPSILON\nomega\n";

        let mut lines: Vec<String> = before.split('\n').map(str::to_string).collect();
        for edit in seam_survey::minimal_edits(before, after).into_iter().rev() {
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

        let message =
            placeholder_checks::refuse_residual_placeholder(original, produced, "modname")
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

    fn moved(name: &str, visibility: &str, outside: bool) -> seam_survey::MovedItem {
        seam_survey::MovedItem {
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
    fn moved_within(name: &str, module: &str, outside: bool) -> seam_survey::MovedItem {
        seam_survey::MovedItem {
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
            module_text::with_facade(text, "rendering", &["pub use rendering::*;".to_string()])
                .unwrap();

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
            module_text::with_facade(text, "rendering", &["pub use rendering::*;".to_string()])
                .unwrap();

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
            visibility::restore_visibility(widened, "rendering", &[moved("normalise", "", false)])
                .unwrap();

        assert!(
            restored.contains("    fn normalise(v: f64) -> f64 { v }"),
            "{restored}"
        );
        assert!(report.is_empty());
    }

    /// An assist that relocates only part of the anchored range rewrites what it leaves behind to
    /// reach into the module it just wrote. The survey feeding the narrowing ran **before** that,
    /// over the original text, where those references were inside the range — so it reports no
    /// outside reach for an item the produced parent now names through the module.
    ///
    /// One live extraction on `parser.rs` ended exactly here: `struct StructuredPlan` and
    /// `fn prd_value_looks_like_md_file_path` were narrowed back to private while the parent had
    /// been rewritten to `planning::StructuredPlan` and
    /// `planning::prd_value_looks_like_md_file_path`. `E0603` at the next build, after the run
    /// reported `applied 1 of 1 operations`. The produced text is the only witness that is not
    /// stale, so it is the one the decision stands on.
    #[test]
    fn keeps_the_widening_of_an_item_the_produced_parent_reaches_through_the_module() {
        // Given a parent the assist rewrote to reach an item through the module it wrote
        let produced = "mod planning {\n    pub(crate) struct StructuredPlan {\n        goal: Option<String>,\n    }\n}\n\nfn parse_planning_response_impl(s: &str) -> u32 {\n    let parsed: planning::StructuredPlan = serde_json::from_str(s).unwrap();\n    0\n}\n";

        // When the survey taken before the assist ran saw no reference from outside the range
        let (restored, report) = visibility::restore_visibility(
            produced,
            "planning",
            &[moved("StructuredPlan", "", false)],
        )
        .unwrap();

        // Then the widening stands, because narrowing it is `E0603` on the line above
        assert!(
            restored.contains("pub(crate) struct StructuredPlan"),
            "{restored}"
        );
        assert_eq!(report.len(), 1);
        assert_eq!(report[0].item, "StructuredPlan");
    }

    /// The narrowing rule itself is right and stays: a helper that travelled with its only caller
    /// keeps the privacy the compiler was enforcing. A fix that simply stopped narrowing would pass
    /// the test above and undo that.
    #[test]
    fn narrows_an_item_the_produced_parent_does_not_reach_through_the_module() {
        // Given a produced parent that names the module but never reaches this item through it
        let produced = "mod planning {\n    pub(crate) fn prd_value_looks_like_md_file_path(p: &str) -> bool {\n        p.ends_with(\".md\")\n    }\n}\n\nfn unrelated() -> u32 {\n    0\n}\n";

        // When the item was written private and nothing outside reaches it
        let (restored, report) = visibility::restore_visibility(
            produced,
            "planning",
            &[moved("prd_value_looks_like_md_file_path", "", false)],
        )
        .unwrap();

        // Then it goes back to private
        assert!(
            restored.contains("    fn prd_value_looks_like_md_file_path(p: &str) -> bool {"),
            "{restored}"
        );
        assert!(report.is_empty());
    }

    /// A mention inside the module's own body is not an outside reach. The module referring to its
    /// own item — or to `planning::` from within `planning` — says nothing about the parent.
    #[test]
    fn reads_no_outside_reach_from_a_mention_inside_the_module_itself() {
        // Given a module whose own body names the item through the module path
        let produced = "mod planning {\n    pub(crate) struct StructuredPlan;\n    fn build() -> planning::StructuredPlan {\n        planning::StructuredPlan\n    }\n}\n\nfn unrelated() -> u32 {\n    0\n}\n";

        // When nothing outside the module reaches it
        let (restored, report) = visibility::restore_visibility(
            produced,
            "planning",
            &[moved("StructuredPlan", "", false)],
        )
        .unwrap();

        // Then the mention inside the module does not hold the widening open
        assert!(
            restored.contains("    struct StructuredPlan;"),
            "{restored}"
        );
        assert!(report.is_empty());
    }

    /// The other half of the partial relocation, and the one the author has to know about.
    /// Visibility is now decided on the produced text, so the result compiles — but it is not what
    /// the plan described. An anchor covering six items that yields a module holding four has left
    /// two behind, reaching into the module through qualified paths, and the seam the author asked
    /// for does not exist. A `#carve` node whose contract is "one module per phase" would report
    /// success and fail its own shape assertions.
    ///
    /// Keyed on a surveyed item rather than on a line count: the survey holds exactly the
    /// path-reachable items the range covered, so an item missing from the produced module is
    /// material by construction, and trailing trivia never trips it.
    #[test]
    fn refuses_an_assist_that_left_an_anchored_item_behind() {
        // Given a produced file whose module holds one of the two items the range covered
        let produced = "mod planning {\n    pub(crate) struct StructuredPlan;\n}\n\nfn parse_planning_response_impl(s: &str) -> planning::StructuredPlan {\n    planning::StructuredPlan\n}\n";
        let anchored = [
            moved("StructuredPlan", "", true),
            moved("parse_planning_response_impl", "", false),
        ];

        // When what the assist wrote is held against what the anchor asked for
        let refusal = visibility::refuse_partial_relocation(produced, "planning", &anchored)
            .expect_err("an item left behind is refused");

        // Then it is a seam refusal naming the item that stayed
        assert!(
            matches!(refusal, RestructureError::SeamRefused(_)),
            "{refusal:?}"
        );
        assert!(
            refusal.to_string().contains("parse_planning_response_impl"),
            "{refusal}"
        );
    }

    #[test]
    fn accepts_a_relocation_that_carried_every_anchored_item() {
        // Given a produced module holding both items the range covered
        let produced = "mod planning {\n    pub(crate) struct StructuredPlan;\n    pub(crate) fn parse(s: &str) -> u32 {\n        0\n    }\n}\n";
        let anchored = [
            moved("StructuredPlan", "", false),
            moved("parse", "", false),
        ];

        // Then nothing is refused
        assert!(refuse_partial_relocation(produced, "planning", &anchored).is_ok());
    }

    /// An item the range held inside an inline module of its own travels nested, so it is declared
    /// deeper in the block rather than at its top level. Looking only at the block's own level
    /// would report every one of them as left behind.
    #[test]
    fn finds_an_anchored_item_the_assist_nested_inside_an_inline_module() {
        // Given a produced module whose own body holds the inline module the range covered
        let produced = "mod grouped {\n    pub(crate) mod inner {\n        pub(crate) fn tier(v: f64) -> u32 {\n            0\n        }\n    }\n}\n";
        let anchored = [moved_within("tier", "inner", false)];

        // Then the nested item counts as relocated
        assert!(refuse_partial_relocation(produced, "grouped", &anchored).is_ok());
    }

    #[test]
    fn keeps_the_widening_of_an_item_something_outside_the_module_reaches() {
        let widened = "mod rendering {\n    pub(crate) fn clamp(v: f64) -> f64 { v }\n}\n";

        let (restored, report) =
            visibility::restore_visibility(widened, "rendering", &[moved("clamp", "", true)])
                .unwrap();

        assert!(restored.contains("pub(crate) fn clamp"), "{restored}");
        assert_eq!(report.len(), 1);
        assert_eq!(report[0].item, "clamp");
        assert_eq!(report[0].from, "private");
        assert_eq!(report[0].to, "pub(crate)");
    }

    #[test]
    fn puts_a_scoped_visibility_back_as_it_was_written() {
        let widened = "mod rendering {\n    pub(crate) fn helper(v: f64) -> f64 { v }\n}\n";

        let (restored, _) = visibility::restore_visibility(
            widened,
            "rendering",
            &[moved("helper", "pub(super)", false)],
        )
        .unwrap();

        assert!(
            restored.contains("    pub(in super::super) fn helper"),
            "{restored}"
        );
    }

    /// The assist never narrows, so an item written `pub` is already as it was and has nothing to
    /// report.
    #[test]
    fn leaves_an_item_that_was_already_public_alone() {
        let text = "mod rendering {\n    pub fn render(v: f64) -> f64 { v }\n}\n";

        let (restored, report) =
            visibility::restore_visibility(text, "rendering", &[moved("render", "pub", true)])
                .unwrap();

        assert_eq!(restored, text);
        assert!(report.is_empty());
    }

    #[test]
    fn names_the_item_and_the_file_whose_reference_would_be_stranded() {
        let mut item = moved("scaled_label", "pub", true);
        item.stranded_in = vec!["src/lib.rs".to_string()];

        let message = seam_survey::refuse_stranded(&[item])
            .unwrap_err()
            .to_string();

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
            visibility::restore_visibility(text, "rendering", &[moved("helper", "", false)])
                .unwrap();

        let lines: Vec<&str> = restored.split('\n').collect();
        assert_eq!(lines[0], "pub(crate) fn helper(v: f64) -> f64 { v }");
        assert_eq!(lines[3], "    fn helper(v: f64) -> f64 { v }");
    }

    /// A reader acts on the class, and every class currently says the same wrong thing. A module
    /// name already taken is a fact about the code the seam is being cut in — the plan asked for
    /// something the file will not permit — and "plan is malformed" sends the author to edit a plan
    /// that is correct. `status.rs` already refuses to let this distinction be lost at the
    /// transport boundary; it has to survive being made.
    #[test]
    fn a_seam_refusal_does_not_tell_the_author_their_plan_is_malformed() {
        // Given a file that already declares the module an extraction wants to write
        let text = "mod grouped;\npub fn foo() -> u32 {\n    1\n}\n";
        let range = Range {
            start: Position { line: 2, col: 1 },
            end: Position { line: 4, col: 2 },
        };

        // When the seam is refused for that collision
        let refusal = module_text::refuse_module_name_taken(text, "grouped", range)
            .expect_err("a taken module name is refused");

        // Then it is a seam refusal, and it does not blame the plan
        assert!(
            matches!(refusal, RestructureError::SeamRefused(_)),
            "{refusal:?}"
        );
        assert!(
            !refusal.to_string().contains("plan is malformed"),
            "{refusal}"
        );
    }

    /// The other half of the same distinction. rust-analyzer writing `_` into a signature is a
    /// defect in the server's answer, and the remedy the message already gives — retry against a
    /// warm server — is not something an author does to a plan.
    #[test]
    fn a_defect_in_the_servers_answer_is_not_reported_as_a_defect_in_the_plan() {
        // Given an extraction rust-analyzer produced before it could infer the signature
        let produced = "fn fun_name(v: _) -> _ {\n    v\n}\n";

        // When the run refuses it
        let refusal = placeholder_checks::refuse_inferred_placeholder(produced, "fn fun_name")
            .expect_err("an inferred placeholder is refused");

        // Then it is a server defect, and it does not blame the plan
        assert!(
            matches!(refusal, RestructureError::ServerDefect(_)),
            "{refusal:?}"
        );
        assert!(
            !refusal.to_string().contains("plan is malformed"),
            "{refusal}"
        );
    }

    /// The class that keeps its name. An operation arriving without the name it needs is a plan
    /// that does not say enough, and editing the plan is exactly the remedy.
    #[test]
    fn a_plan_that_does_not_say_enough_is_still_reported_as_a_malformed_plan() {
        // Given the refusal raised when an operation carries no name
        let refusal = failure("the operation needs a name");

        // Then it stays a malformed plan
        assert!(
            matches!(refusal, RestructureError::MalformedPlan(_)),
            "{refusal:?}"
        );
        assert!(
            refusal.to_string().contains("plan is malformed"),
            "{refusal}"
        );
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

        let message = placeholder_checks::refuse_inferred_placeholder(text, "fn compute_spread")
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
        assert!(carries_placeholder_type("fn f(s: _) {"));
        assert!(carries_placeholder_type("fn f() -> (_, _) {"));
    }

    /// `'_` is an elided lifetime, legal in a parameter's type, and the signature rust-analyzer
    /// writes for a borrowed view (`state: AgentRosterState<'_>`) — not an untyped `_`.
    #[test]
    fn reads_no_placeholder_type_out_of_an_elided_lifetime() {
        assert!(!carries_placeholder_type(
            "fn f(state: AgentRosterState<'_>) -> Result<(), Status> {"
        ));
        assert!(!carries_placeholder_type("fn f(s: &'_ str) {"));
    }

    /// An elided lifetime beside a real placeholder hides nothing: the `_` that is a type is still
    /// read as one.
    #[test]
    fn reads_a_placeholder_type_beside_an_elided_lifetime() {
        assert!(carries_placeholder_type(
            "fn f(state: AgentRosterState<'_>, value: _) {"
        ));
        assert!(carries_placeholder_type("fn f(s: &'_ str) -> Vec<_> {"));
    }

    /// The signature the port-move pilot's cold `check --deep` refused, whose every type was
    /// inferred.
    #[test]
    fn accepts_a_signature_borrowing_a_view_through_an_elided_lifetime() {
        // Given
        let text = "fn agent_clone_for(session_id: &str, agent_id: &str, session_dir: PathBuf, \
                    state: tddy_session_agents::AgentRosterState<'_>) -> Result<AgentClone, Status> {\n";

        // When
        let checked = placeholder_checks::refuse_inferred_placeholder(text, "fn agent_clone_for");

        // Then
        assert!(checked.is_ok(), "{checked:?}");
    }

    // ---- D8: an alias the parent binds ----

    /// `ProbeOutcome as ProtoProbeOutcome` is how every generated proto type in this workspace is
    /// referred to. rust-analyzer offers the unaliased path, which binds nothing.
    #[test]
    fn reads_the_path_behind_an_alias_the_parent_declares() {
        let text = "use tddy_service::proto::host::ProbeOutcome as ProtoProbeOutcome;\n\
                    mod host_messages {\n\
                        fn f(o: &ProtoProbeOutcome) {}\n\
                    }\n";

        assert_eq!(
            alias_target(text, "host_messages", "ProtoProbeOutcome").as_deref(),
            Some("tddy_service::proto::host::ProbeOutcome")
        );
    }

    /// An alias inside the extracted module already binds the name there, so it is not the
    /// parent's declaration and reconstructing it would be a duplicate binding.
    #[test]
    fn ignores_an_alias_declared_inside_the_extracted_module() {
        let text = "mod host_messages {\n\
                        use x::Y as Z;\n\
                    }\n";

        assert_eq!(alias_target(text, "host_messages", "Z"), None);
    }

    #[test]
    fn reads_an_alias_out_of_a_grouped_use_tree() {
        let bindings = module_text::aliased_bindings("use a::b::{C as D, E, F as G};\n");

        assert_eq!(
            bindings,
            vec![
                ("D".to_string(), "a::b::C".to_string()),
                ("G".to_string(), "a::b::F".to_string())
            ]
        );
    }

    /// `as _` imports a trait without binding a name, so nothing can be unresolved under it.
    #[test]
    fn records_no_binding_for_an_anonymous_import() {
        assert!(aliased_bindings("use prost::Message as _;\n").is_empty());
    }

    #[test]
    fn writes_the_reconstructed_import_as_the_modules_first_line() {
        let text = "mod m {\n    fn f() {}\n}\n";

        let out = module_text::with_module_import(text, "m", "use x::Y as Z;").unwrap();

        assert_eq!(out, "mod m {\n    use x::Y as Z;\n    fn f() {}\n}\n");
    }

    /// A seam carrying something `pub` still publishes it.
    #[test]
    fn reexports_a_glob_at_pub_when_the_seam_moved_something_public() {
        let items = [moved("Visible", "pub", true), moved("Hidden", "", false)];

        assert_eq!(
            facade_lines("rendering", &items, Reexport::Glob).unwrap(),
            ["pub use rendering::*;"]
        );
    }

    /// The assist rewrites what it relocates to `pub(crate)`, so a seam of private items yields a
    /// `pub` glob that re-exports nothing — which `-D warnings` turns into a build failure.
    #[test]
    fn reexports_a_glob_at_pub_crate_when_nothing_the_seam_moved_is_public() {
        let items = [
            moved("Hidden", "", false),
            moved("AlsoHidden", "pub(crate)", true),
        ];

        assert_eq!(
            facade_lines("rendering", &items, Reexport::Glob).unwrap(),
            ["pub(crate) use rendering::*;"]
        );
    }

    // ---- D7: the facade the import pass used to defeat ----

    /// A glob facade re-exports everything the module holds, so the parent needs no named import
    /// for any of it — and a named one would be private and would shadow the facade.
    #[test]
    fn treats_a_glob_facade_as_binding_every_moved_name() {
        let moved = [moved("SeededAgentClones", "pub", true)];

        assert!(facade_will_bind(
            "SeededAgentClones",
            &moved,
            Reexport::Glob
        ));
    }

    /// A named facade covers only what something outside the seam reaches, so a purely internal
    /// item still needs its import.
    #[test]
    fn treats_a_named_facade_as_binding_only_what_it_reexports() {
        let moved = [moved("Internal", "", false)];

        assert!(!facade_will_bind("Internal", &moved, Reexport::Named));
    }

    #[test]
    fn treats_no_facade_as_binding_nothing() {
        let moved = [moved("SeededAgentClones", "pub", true)];

        assert!(!facade_will_bind(
            "SeededAgentClones",
            &moved,
            Reexport::None
        ));
    }

    // ---- D6: a rewrite written over itself ----

    /// The exact corruption one real run produced and reported success over.
    #[test]
    fn refuses_a_qualified_path_whose_identifier_was_written_over_itself() {
        let moved = [moved("SeededCloneGuard", "pub", true)];
        let text =
            "    ) -> Result<seeded_clone_guard::SeededCloneGuardloneGuardloneGuard, Status> {\n";

        let outcome = facade::refuse_mangled_rewrite(text, "seeded_clone_guard", &moved);

        let message = match outcome {
            Err(RestructureError::ServerDefect(m)) => m,
            other => panic!("expected a refusal, got {other:?}"),
        };
        assert!(message.contains("SeededCloneGuard"), "{message}");
        assert!(message.contains("written over itself"), "{message}");
    }

    #[test]
    fn accepts_a_qualified_path_naming_something_the_seam_moved() {
        let moved = [moved("SeededCloneGuard", "pub", true)];
        let text = "    ) -> Result<seeded_clone_guard::SeededCloneGuard, Status> {\n";

        assert!(refuse_mangled_rewrite(text, "seeded_clone_guard", &moved).is_ok());
    }

    /// A path through a different module was not written by this operation, so it is not weighed —
    /// including one whose qualifier merely ends with this module's name.
    #[test]
    fn ignores_a_path_through_another_module() {
        let moved = [moved("Guard", "pub", true)];
        let text = "    let a = other::guard::GuardSomethingElse::new();\n";

        assert!(refuse_mangled_rewrite(text, "guard", &moved).is_ok());
    }

    /// rust-analyzer offers `Import` for items, never for a bare module path, so a name reached
    /// through `use crate::tool_engine;` had nothing on offer and was skipped in silence.
    #[test]
    fn reads_a_module_binding_the_parent_declares() {
        let text = "use crate::tool_engine;\n\
                    mod svc {\n\
                        fn f() { tool_engine::execute_tool(); }\n\
                    }\n";

        assert_eq!(
            parent_binding(text, "svc", "tool_engine").as_deref(),
            Some("crate::tool_engine")
        );
    }

    #[test]
    fn reads_a_plain_item_binding_the_parent_declares() {
        let text = "use a::b::Thing;\nmod svc {\n    fn f(t: Thing) {}\n}\n";

        assert_eq!(
            parent_binding(text, "svc", "Thing").as_deref(),
            Some("a::b::Thing")
        );
    }

    /// A binding inside the module already provides the name there.
    #[test]
    fn ignores_a_binding_declared_inside_the_module() {
        let text = "mod svc {\n    use crate::tool_engine;\n}\n";

        assert_eq!(parent_binding(text, "svc", "tool_engine"), None);
    }

    /// After a seam moves the code that used a name, the import pass prunes the parent's binding —
    /// so the name itself is in scope nowhere, while the module it came from still is.
    #[test]
    fn settles_a_contested_name_on_the_module_the_file_already_imports_from() {
        let offered = [
            "Import `tddy_service::proto::session::Signal`",
            "Import `sysinfo::Signal`",
            "Import `tokio::signal::unix::Signal`",
        ];

        let chosen = import_text::choose_import(
            "use tddy_service::proto::catalog::{ListToolsRequest};
use tddy_service::proto::session::{StartSessionResponse};\n",
            &offered,
        );

        assert_eq!(
            chosen,
            Some("Import `tddy_service::proto::session::Signal`")
        );
    }

    /// Two candidates from two imported modules is the ambiguity this refuses, not one it guesses at.
    #[test]
    fn settles_nothing_when_two_candidates_come_from_imported_modules() {
        let offered = ["Import `a::b::Thing`", "Import `c::d::Thing`"];

        let chosen = import_text::choose_import("use a::b::Other;\nuse c::d::Another;\n", &offered);

        assert_eq!(chosen, None);
    }

    /// An exact binding still wins over mere module agreement.
    #[test]
    fn prefers_an_exact_binding_over_module_agreement() {
        let offered = ["Import `a::b::Thing`", "Import `c::d::Thing`"];

        let chosen = import_text::choose_import("use c::d::Thing;\nuse a::b::Other;\n", &offered);

        assert_eq!(chosen, Some("Import `c::d::Thing`"));
    }

    /// A crate that re-exports an item at its root gives one item two paths, and rust-analyzer
    /// offers the shortest. `tddy-core` publishes `pub use error::{BackendError, ParseError,
    /// WorkflowError};`, so a file writing the canonical `tddy_core::error::ParseError` is offered
    /// `tddy_core::ParseError` — a different string for the same type. Neither the exact-path tier
    /// nor the module tier can see that, and one live extraction was refused three candidates deep
    /// over it. The crate the file already binds the name from is the evidence that settles it.
    #[test]
    fn settles_a_contested_name_on_the_crate_the_file_already_binds_it_from() {
        // Given the three paths rust-analyzer offered for `ParseError`, one of them a re-export
        let offered = [
            "Import `tddy_core::ParseError`",
            "Import `std::string::ParseError`",
            "Import `chrono::ParseError`",
        ];

        // When the file that lost the name binds it canonically from one of those crates
        let chosen = import_text::choose_import("use tddy_core::error::ParseError;\n", &offered);

        // Then the candidate rooted in that same crate is the one the moved code meant
        assert_eq!(chosen, Some("Import `tddy_core::ParseError`"));
    }

    /// Two candidates rooted in the crate the name is bound from is an ambiguity the crate root
    /// cannot settle — the tier narrows by crate, and a crate holding both is no narrower.
    #[test]
    fn settles_nothing_when_two_candidates_share_the_crate_the_name_is_bound_from() {
        // Given two candidates from one crate and one from another
        let offered = [
            "Import `tddy_core::ParseError`",
            "Import `tddy_core::json::ParseError`",
            "Import `chrono::ParseError`",
        ];

        // When the file binds the name from the crate that offers two of them
        let chosen = import_text::choose_import("use tddy_core::error::ParseError;\n", &offered);

        // Then nothing is chosen, because the crate does not say which of its two was meant
        assert_eq!(chosen, None);
    }

    /// The crate tier keys on a binding **of the same name**, not on any binding from that crate.
    /// Keyed on any binding it would be worthless here: almost every file imports something from
    /// `std`, so `std::string::ParseError` would match as readily as the one the file means, and the
    /// tier would refuse every time it was needed.
    #[test]
    fn keys_the_crate_tier_on_a_binding_of_the_contested_name() {
        // Given two candidates, one rooted in a crate this file imports other names from
        let offered = [
            "Import `tddy_core::ParseError`",
            "Import `std::string::ParseError`",
        ];

        // When the file imports something unrelated from `std` and binds the name from `tddy_core`
        let text = "use std::collections::HashMap;\nuse tddy_core::error::ParseError;\n";
        let chosen = import_text::choose_import(text, &offered);

        // Then the `std` import is not evidence about `ParseError`, and the binding of that name is
        assert_eq!(chosen, Some("Import `tddy_core::ParseError`"));
    }

    #[test]
    fn reads_the_module_a_path_lives_in() {
        assert_eq!(parent_module("a::b::C").as_deref(), Some("a::b"));
        assert_eq!(parent_module("C"), None);
    }

    /// A timeout has to say how far the index got. The server's *last* notification is often a
    /// sub-step with no percentage, so the furthest percentage is tracked separately.
    #[test]
    fn reports_how_far_the_index_got_even_when_the_last_line_has_no_percentage() {
        // Given a phase that counted to 64% and then emitted a sub-step with no number
        let mut chatter = ServerChatter::default();
        chatter.absorb(&json!({
            "method": "$/progress",
            "params": { "token": "t", "value": { "kind": "begin", "title": "roots scanned" } }
        }));
        chatter.absorb(&json!({
            "method": "$/progress",
            "params": { "token": "t", "value": { "kind": "report", "percentage": 64 } }
        }));
        chatter.absorb(&json!({
            "method": "$/progress",
            "params": { "token": "t", "value": { "kind": "report", "message": "tddy_desktop (lib)" } }
        }));

        // Then the account carries both the sub-step and the number the sub-step lacks
        let how_far = chatter.how_far();
        assert!(how_far.contains("tddy_desktop (lib)"), "{how_far}");
        assert!(how_far.contains("64%"), "{how_far}");
    }

    /// A stall at 12% and a timeout at 99% want opposite responses, so the number must not be the
    /// first one seen.
    #[test]
    fn keeps_the_furthest_percentage_rather_than_the_first() {
        let mut chatter = ServerChatter::default();
        for pct in [10, 55, 91] {
            chatter.absorb(&json!({
                "method": "$/progress",
                "params": { "token": "t", "value": { "kind": "report", "percentage": pct } }
            }));
        }

        assert!(chatter.how_far().contains("91%"), "{}", chatter.how_far());
    }

    #[test]
    fn says_nothing_was_reported_when_the_server_was_silent() {
        assert_eq!(ServerChatter::default().how_far(), "nothing reported");
    }

    /// A backend with no server behind it. Every wait in this file is decided before a request is
    /// sent, so what ends one is answerable without a language server at all.
    fn a_backend() -> RustBackend {
        RustBackend::new("/usr/bin/rust-analyzer", "/tmp", "/tmp")
    }

    fn a_server_status_saying_quiescent(quiescent: bool) -> Value {
        json!({
            "method": "experimental/serverStatus",
            "params": { "health": "ok", "quiescent": quiescent }
        })
    }

    #[test]
    fn an_empty_outline_is_not_believed_before_the_server_has_been_seen_to_load() {
        // Given a server that has said it is still loading
        let mut backend = a_backend();
        backend
            .chatter
            .absorb(&a_server_status_saying_quiescent(false));

        // When it answers with no symbols
        let believed = backend.outline_is_the_servers_answer(&json!([]));

        // Then that is the silence of a loading server, not a file that defines nothing
        assert!(!believed);
    }

    #[test]
    fn an_empty_outline_is_believed_once_the_server_has_been_seen_to_finish_loading() {
        // Given a server that has reported itself quiescent
        let mut backend = a_backend();
        backend
            .chatter
            .absorb(&a_server_status_saying_quiescent(true));

        // When it answers with no symbols
        let believed = backend.outline_is_the_servers_answer(&json!([]));

        // Then the file genuinely defines nothing, and nothing waits for more
        assert!(believed);
    }

    #[test]
    fn an_outline_with_items_is_believed_whenever_it_arrives() {
        // Given a server that has not said anything about its state
        let backend = a_backend();

        // When it answers with an item
        let believed = backend.outline_is_the_servers_answer(&json!([{ "name": "Queue" }]));

        // Then it is the real answer
        assert!(believed);
    }

    /// The replacement for the budgets: nothing but the caller ends a wait, and it ends it at once
    /// rather than at the end of the poll the wait was sleeping out.
    #[test]
    fn stops_waiting_as_soon_as_its_caller_does() {
        // Given a backend whose caller has stopped waiting
        let cancel = CancellationToken::new();
        let backend = a_backend().with_cancellation(cancel.clone());
        cancel.cancel();

        // When it would sleep out a poll interval far longer than any test
        let started = Instant::now();
        let keep_waiting = backend.keep_waiting(Duration::from_secs(300));

        // Then it does not wait at all, and reports the wait as over
        assert!(!keep_waiting, "a cancelled wait asked to continue");
        assert!(
            // Wall-clock, so an exact figure is not available; the poll it skipped is 300s.
            started.elapsed() < Duration::from_secs(1),
            "a cancelled wait slept for {:?}",
            started.elapsed()
        );
    }

    /// The other half of the same rule: while the caller is still waiting, the server is left
    /// alone for the whole poll interval rather than asked again immediately.
    #[test]
    fn waits_out_the_whole_poll_while_its_caller_is_still_waiting() {
        // Given a backend whose caller is still waiting
        let backend = a_backend().with_cancellation(CancellationToken::new());

        // When it sleeps out a poll interval
        let started = Instant::now();
        let keep_waiting = backend.keep_waiting(Duration::from_millis(300));

        // Then it waited the interval and reports the wait as continuing
        assert!(keep_waiting, "an uncancelled wait reported itself over");
        assert!(
            // Wall-clock again: the floor is the interval, and a loaded machine may exceed it.
            started.elapsed() >= Duration::from_millis(300),
            "the poll returned early, after {:?}",
            started.elapsed()
        );
    }

    /// A backend nobody handed a token to waits on readiness alone — which is what a single-shot
    /// caller whose process *is* the operation means, and is why no wait needs a budget.
    #[test]
    fn waits_on_readiness_alone_when_no_caller_handed_it_a_token() {
        // Given a backend built with no cancellation token
        let backend = a_backend();

        // When it is asked whether a wait may continue
        // Then it may, because nothing has said otherwise
        assert!(backend.keep_waiting(Duration::ZERO));
    }

    /// The error class the TODO called wrong: the plan was not malformed, the indexer never
    /// settled, and the two want opposite responses from whoever reads the refusal.
    #[test]
    fn names_the_server_rather_than_the_plan_when_a_method_never_settles() {
        // Given a method the server never settled enough to answer
        let error = unsettled(
            "textDocument/codeAction",
            Duration::from_secs(6),
            "working (100%)".to_string(),
        );

        // Then the refusal names the method, the wait and where the index got to
        match error {
            RestructureError::ServerNotSettled {
                method,
                seconds,
                last,
            } => {
                assert_eq!(method, "textDocument/codeAction");
                assert_eq!(seconds, 6);
                assert_eq!(last, "working (100%)");
            }
            other => panic!("expected ServerNotSettled, got {other:?}"),
        }
    }

    /// A server that is ready here and offers nothing is a seam refusal; the reader should look at
    /// the range they asked for.
    #[test]
    fn names_the_absent_assist_when_the_server_is_ready_to_answer() {
        // Given a server that can type the range and still offers something else
        let error = absent_assist(
            "extract into function",
            &["Extract into variable".to_string()],
        );

        // Then the failure points at the range
        match error {
            RestructureError::SeamRefused(message) => {
                assert!(message.contains("extract into function"), "{message}");
                assert!(message.contains("given range"), "{message}");
                // The evidence that separates a wrong title from an unrefactorable range.
                assert!(message.contains("Extract into variable"), "{message}");
            }
            other => panic!("expected SeamRefused, got {other:?}"),
        }
    }

    /// The case that cost hours: the server answers `codeAction` from the syntax tree while it
    /// still cannot type the range, so an assist needing inference is absent for a reason that
    /// has nothing to do with the range. Reported as an absent assist, it reads as a plan defect.
    #[test]
    fn reports_an_incomplete_index_when_the_range_could_not_be_typed() {
        // Given a cancelled wait on a server that answered with syntax-level assists but could
        // not type the range
        let error = incomplete_assist_index(
            "extract into function",
            &["Extract into variable".to_string()],
            Some(false),
            Duration::from_secs(120),
            "working (100%)".to_string(),
            "cargo 1.94".to_string(),
        );

        // Then it is an indexing problem, and it says why the assist was never offered
        match error {
            RestructureError::IndexingIncomplete { seconds, last, .. } => {
                assert_eq!(seconds, 120);
                assert!(last.contains("could not type the range"), "{last}");
                assert!(last.contains("needs type inference"), "{last}");
                assert!(last.contains("Extract into variable"), "{last}");
            }
            other => panic!("expected IndexingIncomplete, got {other:?}"),
        }
    }

    /// A wait cancelled while the graph was still loading is an index that was not ready, and the
    /// reader needs where it got to rather than anything about the anchors.
    #[test]
    fn reports_an_incomplete_index_when_a_wait_is_cancelled_before_the_graph_loads() {
        // Given a cancelled wait on an assist that needs no inference, so nothing was probed
        let error = incomplete_assist_index(
            "extract into function",
            &[],
            None,
            Duration::from_secs(45),
            "discovering sysroot".to_string(),
            "cargo 1.94".to_string(),
        );

        // Then the failure names how long it waited and where the server got to
        match error {
            RestructureError::IndexingIncomplete {
                seconds,
                last,
                environment,
            } => {
                assert_eq!(seconds, 45);
                assert_eq!(last, "discovering sysroot");
                assert_eq!(environment, "cargo 1.94");
            }
            other => panic!("expected IndexingIncomplete, got {other:?}"),
        }
    }

    fn block_of(text: &str) -> (Vec<String>, module_text::ModuleBlock) {
        let source: Vec<String> = text.split('\n').map(str::to_string).collect();
        let block = module_text::module_bounds(&source, "moved").expect("a `mod moved` block");
        (source, block)
    }

    fn unresolved_at(line: usize, text: &str) -> import_text::UnresolvedName {
        import_text::UnresolvedName {
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

        let kept = import_text::without_dead_imports(
            &source,
            &block,
            &[unresolved_at(1, "new_with_config")],
        );

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

        let kept = import_text::without_dead_imports(&source, &block, &[]);

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

        let kept = import_text::without_dead_imports(&source, &block, &[]);

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

        let kept = import_text::without_dead_imports(&source, &block, &[]);

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

        let kept = import_text::without_dead_imports(&source, &block, &[unresolved_at(0, "stale")]);

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

    /// A module that received the parent's rebuilt `use … as …` for an alias.
    fn a_module_importing(declaration: &str) -> String {
        format!("mod readings {{\n    {declaration}\n\n    fn go() {{}}\n}}\n")
    }

    /// E1: `use a::B as C;` binds `C`. Read as binding `B`, the alias branch never sees the line it
    /// has just written, and writes it again on every pass until the backstop.
    #[test]
    fn finds_an_alias_the_module_already_binds() {
        // Given
        let text = a_module_importing("use crate::proto::Event as StartSessionEventKind;");

        // When
        let bound = import_text::already_bound(&text, "readings", "StartSessionEventKind").unwrap();

        // Then
        assert!(
            bound,
            "the module's own `as` import was not seen binding its alias"
        );
    }

    /// The other half of the same misreading: an aliased import does not bind the name it renames.
    #[test]
    fn does_not_count_the_renamed_name_as_bound_by_an_alias() {
        // Given
        let text = a_module_importing("use crate::proto::Event as StartSessionEventKind;");

        // When
        let bound = import_text::already_bound(&text, "readings", "Event").unwrap();

        // Then
        assert!(
            !bound,
            "`Event` was counted as bound, though only its alias is"
        );
    }

    /// An alias inside a nested group binds its alias like any other.
    #[test]
    fn finds_an_alias_bound_inside_a_nested_group() {
        // Given
        let text = a_module_importing(
            "use crate::proto::{session::Event as StartSessionEventKind, Signal};",
        );

        // When
        let bound = import_text::already_bound(&text, "readings", "StartSessionEventKind").unwrap();

        // Then
        assert!(
            bound,
            "an alias inside a nested group was not seen as bound"
        );
    }

    /// Guard: the plain member beside an aliased one in the same group is still bound.
    #[test]
    fn still_finds_a_plain_name_beside_an_alias_in_a_group() {
        // Given
        let text = a_module_importing(
            "use crate::proto::{session::Event as StartSessionEventKind, Signal};",
        );

        // When
        let bound = import_text::already_bound(&text, "readings", "Signal").unwrap();

        // Then
        assert!(
            bound,
            "the plain group member stopped being recognised as bound"
        );
    }

    /// `as _` imports a trait for its methods and binds no name at all, so it cannot be the binding
    /// that makes a later import of the trait's name `E0252`.
    #[test]
    fn does_not_count_an_underscore_import_as_binding_its_name() {
        // Given
        let text = a_module_importing("use std::fmt::Write as _;");

        // When
        let bound = import_text::already_bound(&text, "readings", "Write").unwrap();

        // Then
        assert!(!bound, "`use … as _;` was counted as binding `Write`");
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

        let message = facade::facade_lines("grouped", &items, Reexport::Named)
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
        let note = facade::empty_facade_note("grouped", &[], Reexport::Named)
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

        let message = module_text::refuse_module_name_taken(text, "report", lines(3, 4))
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
        let started = std::time::Instant::now();

        chatter.absorb_at(
            &json!({
                "method": "$/progress",
                "params": {
                    "token": "rustAnalyzer/cachePriming",
                    "value": { "kind": "begin", "title": "Priming caches", "cancellable": false }
                }
            }),
            started,
        );
        let line = chatter
            .absorb_at(
                &json!({
                    "method": "$/progress",
                    "params": {
                        "token": "rustAnalyzer/cachePriming",
                        "value": { "kind": "report", "message": "20/28 (serde_core)", "percentage": 71 }
                    }
                }),
                started + std::time::Duration::from_secs(3),
            )
            .expect("a progress report, once the interval has passed, says something worth printing");

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

        let message = module_text::refuse_split_attribute_paths(&text, lines(1, 5))
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
        let found = seam_survey::items_relocated_within(&outline(), whole_file());

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

    /// A member of the `impl` the outline names `holder`, which a sibling left behind calls from
    /// line 9.
    fn a_member_called_from_behind(name: &str, holder: &str) -> seam_survey::MovedItem {
        let mut item = moved(name, "pub", true);
        item.within = vec![holder.to_string()];
        item.referenced_in_impl_at = vec![9];
        item
    }

    /// E3: the assist writes an inherent member as `mod … { use super::Gauge; impl Gauge { … } }`,
    /// so it stays a method of `Gauge`, and `self.doubled()` from the half left behind still
    /// resolves through the type.
    #[test]
    fn accepts_a_seam_whose_inherent_impl_sibling_calls_what_it_moves() {
        // Given
        let items = [a_member_called_from_behind("doubled", "impl Gauge")];

        // When
        let verdict = refuse_impl_sibling_references(&items);

        // Then
        assert!(
            verdict.is_ok(),
            "an inherent method called through `self` was refused: {:?}",
            verdict.err().map(|refusal| refusal.to_string())
        );
    }

    /// Guard: half of a trait `impl` really cannot move. The new module would hold a second
    /// `impl Meter for Gauge` (E0119), and each half would lack the other's items (E0046).
    #[test]
    fn still_refuses_a_seam_that_splits_a_trait_impl() {
        // Given
        let items = [a_member_called_from_behind(
            "doubled",
            "impl Meter for Gauge",
        )];

        // When
        let verdict = refuse_impl_sibling_references(&items);

        // Then
        assert!(verdict.is_err(), "half of a trait `impl` was accepted");
    }

    /// The widening the report has never mentioned, because `restore_visibility` iterates only what
    /// the path-reached survey returned and that survey stops above an `impl`.
    #[test]
    fn reports_a_private_impl_member_the_assist_widened() {
        let (source, block) = block_of(
            "mod moved {\n    impl Dial {\n        pub(crate) fn dial_offset(&self) -> f64 {\n            0.0\n        }\n    }\n}\n",
        );

        let widened = facade::impl_widenings(&source, &block, &[moved("dial_offset", "", true)]);

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

        let offset = lsp_edits::offset_of(text, point);

        assert_eq!(
            position_at(text, offset),
            json!({ "line": point.line, "character": point.character })
        );
    }

    /// The emoji's own first byte, which both units have to agree names the character's start.
    #[test]
    fn resolves_the_offset_an_astral_character_begins_at() {
        let text = "let label = \"\u{1F600}\";\n";

        let offset = lsp_edits::offset_of(
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

        let message =
            placeholder_checks::refuse_residual_placeholder(original, produced, "modname")
                .unwrap_err()
                .to_string();

        assert!(
            message.contains("before the items that reference it"),
            "{message}"
        );
    }

    /// The leftover can equally sit inside a module the file already had, such as its `mod tests`,
    /// which no ordering of the plan changes; lexically the two look alike, so the advice names both.
    #[test]
    fn names_a_module_the_file_already_had_when_the_leftover_sits_in_a_module() {
        // Given a file whose own `mod tests` the assist left calling through the placeholder
        let original = "mod tests {\n    fn a() -> u32 { base() }\n}\n";
        let produced = "mod tests {\n    fn a() -> u32 { modname::base() }\n}\n";

        // When the leftover is refused
        let message =
            placeholder_checks::refuse_residual_placeholder(original, produced, "modname")
                .unwrap_err()
                .to_string();

        // Then the refusal names a module the file already had, for which no ordering helps
        assert!(
            message.contains(
                "If the file already had that module, such as its `mod tests`, no ordering helps: \
                 reach the item there through `use super::*;`, or cut the seam where that module \
                 does not name it."
            ),
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

        let message =
            placeholder_checks::refuse_residual_placeholder(original, produced, "modname")
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

        let described = server_process::describe_server_environment(
            Path::new("/ra/rust-analyzer"),
            "1.93.1",
            &bin,
        );

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

        let described = server_process::describe_server_environment(
            Path::new("/ra/rust-analyzer"),
            "1.93.1",
            &prefix.join("bin"),
        );

        assert!(described.contains("rust-src=present"));
    }
}

#[cfg(test)]
mod cross_file_edit_tests {
    use super::*;

    /// A rust-analyzer rename response naming two documents: the anchor's own, and a caller.
    fn a_rename_touching_two_files() -> Value {
        json!({
            "documentChanges": [
                {
                    "textDocument": { "uri": "file:///repo/src/host_registry.rs", "version": 0 },
                    "edits": [{
                        "range": { "start": { "line": 4, "character": 11 },
                                   "end":   { "line": 4, "character": 23 } },
                        "newText": "HostRegistryStore"
                    }]
                },
                {
                    "textDocument": { "uri": "file:///repo/src/runtime.rs", "version": 0 },
                    "edits": [{
                        "range": { "start": { "line": 91, "character": 20 },
                                   "end":   { "line": 91, "character": 32 } },
                        "newText": "HostRegistryStore"
                    }]
                }
            ]
        })
    }

    /// rust-analyzer computes cross-file rename edits; this backend threw every document but the
    /// anchor's away, so a rename of a symbol referenced elsewhere left those callers naming a
    /// symbol that no longer exists — silently, because the anchor's own file looked correct.
    ///
    /// This is the defect `move_module_to_crate` cannot be built on top of: re-pointing a caller
    /// *is* editing another document.
    #[test]
    fn keeps_every_document_a_rename_touches() {
        // Given
        let response = a_rename_touching_two_files();

        // When
        let edits =
            lsp_edits::workspace_edits_for(&response).expect("a rename response is readable");

        // Then
        let touched: Vec<&str> = edits.iter().map(|(uri, _)| uri.as_str()).collect();
        assert_eq!(
            touched,
            vec![
                "file:///repo/src/host_registry.rs",
                "file:///repo/src/runtime.rs"
            ],
            "a rename must carry the caller's document as well as the anchor's"
        );
    }

    /// The caller's edit has to survive intact, not merely be counted: an empty edit list for a
    /// document is the same silence the old filter produced.
    #[test]
    fn carries_the_callers_own_edit_not_just_its_path() {
        // Given
        let response = a_rename_touching_two_files();

        // When
        let edits =
            lsp_edits::workspace_edits_for(&response).expect("a rename response is readable");

        // Then
        let (_, caller) = edits
            .iter()
            .find(|(uri, _)| uri.ends_with("runtime.rs"))
            .expect("the caller's document is present");
        assert_eq!(caller.len(), 1);
        assert_eq!(caller[0].new_text, "HostRegistryStore");
        assert_eq!(caller[0].start.line, 91);
    }

    /// A response with no documents at all is still an error, as it was before — the fix widens what
    /// counts as an answer, it does not make silence acceptable.
    #[test]
    fn still_refuses_a_response_naming_no_documents() {
        // Given
        let response = json!({ "documentChanges": [] });

        // When
        let outcome = lsp_edits::workspace_edits_for(&response);

        // Then
        assert!(
            outcome.is_err(),
            "an empty rename is not a successful rename"
        );
    }
}

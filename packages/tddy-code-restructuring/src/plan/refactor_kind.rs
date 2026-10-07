use serde::{Deserialize, Serialize};

/// The operations a plan may contain.
///
/// There is deliberately no `CreateFile`, `InsertText`, or `DeleteRange`: files are *caused* by
/// refactors, and code text is produced by language engines.
///
/// Every variant is backed by a real assist in at least one engine — verified by asking each engine
/// what it offers rather than by assumption. `inline_symbol` and `rewrite_import_path` were dropped
/// for having no engine behind them; a vocabulary that advertises what cannot be performed is worse
/// than a smaller one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RefactorKind {
    /// TypeScript `Extract Symbol`/`function_scope`; rust-analyzer `Extract into function`.
    ExtractMethod,
    /// TypeScript `Extract Symbol`/`constant_scope`; rust-analyzer `Extract into variable`.
    ExtractVariable,
    /// TypeScript `Extract type`. No rust-analyzer equivalent.
    ExtractType,
    /// TypeScript `Move to file`. rust-analyzer has no whole-symbol move.
    MoveSymbol,
    /// TypeScript, which rewrites every importer as part of the move.
    MoveFile,
    /// Both engines, via `textDocument/rename` semantics.
    RenameSymbol,
    /// rust-analyzer `Extract Module` — Rust's file-splitting primitive.
    ExtractModuleToFile,
    /// TypeScript.
    OrganizeImports,
    /// TypeScript.
    AddMissingImports,
    /// Moves whole class members into a class of their own. The only operation in this vocabulary
    /// with no engine behind it: TypeScript ships neither extract-class nor move-member, so the
    /// sidecar performs the transformation itself. See `AGENTS.md` for what that costs.
    ExtractClass,
    /// rust-analyzer `Extract Module` over a selection — groups loose items into an inline `mod`,
    /// which is a different assist from the `ExtractModuleToFile` above.
    ExtractModule,
    /// rust-analyzer `Generate trait from impl`. TypeScript offers nothing equivalent on a class.
    ExtractTrait,
    /// rust-analyzer `Inline into all callers`. TypeScript has no inline refactor at all.
    InlineMethod,
    /// Moves a module's file into another crate, rewrites its own `use` header, and re-points every
    /// caller. The **third** operation in this vocabulary with no engine behind it, after
    /// `extract_class` and the facade `use` line — rust-analyzer has no cross-crate move assist, so
    /// there is nothing to delegate to.
    ///
    /// It is engine-*informed* rather than engine-*performed*: every caller it rewrites comes from a
    /// real `textDocument/references` result, not a text search. That is the line this operation
    /// holds, and it is why it is not the `rewrite_import_path` the vocabulary already rejected —
    /// that one had no way to find what to rewrite.
    ///
    /// Takes `to` (the destination crate's directory) and honours a crate-level `reexport`, which
    /// leaves `pub use <new_crate>::…;` behind so a move can have zero caller diff — the same
    /// principle as `extract_module`'s facade, one level up.
    MoveModuleToCrate,
    /// Moves **a set of modules** to one crate as a single operation — the same transformation as
    /// `move_module_to_crate` over more than one module, applied all or not at all.
    ///
    /// A mutually-referencing set cannot move one module at a time: the first operation re-points a
    /// sibling's `crate::` path at the crate it is itself about to leave, and between the first
    /// operation and the last the tree does not compile. Naming the whole set in one operation is
    /// what lets a `crate::<sibling>` path that is coming along be told from one staying behind.
    ///
    /// The anchor is the first member and `also` names the rest. One operation is one journal
    /// entry and one edit, so `--resume`, `--from` and `--stop-after` keep meaning what they mean:
    /// a cluster is never half applied.
    MoveClusterToCrate,
    /// Moves a **test binary** — `<crate>/tests/<name>.rs` — to the crate whose code it exercises.
    ///
    /// A sibling of [`Self::MoveModuleToCrate`] rather than a generalisation of it, because a test
    /// binary is a different shape in the two ways that matter, and both make it *simpler*:
    ///
    /// - **Cargo auto-discovers `tests/*.rs`**, so there is no `mod` declaration anywhere to find,
    ///   remove or rewrite. A module move's whole `left_behind` pass has nothing to do here.
    /// - **Nothing can reference a test binary**, so there is no caller to keep resolving and
    ///   therefore no facade. `reexport` is refused rather than ignored.
    ///
    /// What it does share is the header pass — and it needs the strongest form of it, because a
    /// test's `use` path may reach its subject through **two** re-export facades before it lands on
    /// the crate that defines the item.
    ///
    /// Takes `to`, the destination crate's directory. The destination's `[dev-dependencies]` gain
    /// what the moved test names, not its `[dependencies]`.
    MoveTestBinaryToCrate,
    /// Moves a run of items into **another existing module of the same crate**, in any file.
    ///
    /// Not [`Self::moves_across_crates`]: nothing leaves the crate, so there is no manifest to edit
    /// and no crate to depend on. Like the cross-crate moves it has no assist behind it (rust-analyzer
    /// has no "move item to another module"), so it is engine-*informed*: every caller it re-points
    /// comes from `textDocument/references`. Anchored by `items` (or a single `item`); `to` is the
    /// destination module's path, rooted at the package name, and must already exist.
    ///
    /// `reexport: glob`/`named` leave a `pub use` where the items were and re-point no caller;
    /// `none` (or absent) re-points every caller, which is the difference from `extract_module`,
    /// where `none` refuses when another file reaches the items.
    MoveItem,
    /// Moves a module, with the directory of its children, under **another existing module of the
    /// same crate**.
    ///
    /// Anchored by an `items` (or single `item`) anchor on the module's `mod` declaration in its old
    /// parent; `to` is the new parent's module path, rooted at the package name, and must already
    /// exist. The module's files are moved with `git mv`, the declaration travels with its visibility
    /// and attributes, and every path that named the module is re-pointed from the server's reference
    /// set. Not [`Self::moves_across_crates`], for the reason [`Self::MoveItem`] is not.
    ///
    /// `reexport: glob` leaves `pub use <new parent>::<module>;` in the old parent and re-points no
    /// caller; `none` (or absent) re-points every caller. `named` is refused: a module has no
    /// per-item facade, and `glob` is the word for the one a module needs.
    ReparentModule,
    /// rust-analyzer `Remove unused parameter`: drops a parameter the body never reads, from the
    /// declaration and from every call site. Anchored on the function; `name` is the parameter.
    ///
    /// The server offers it only for an unused parameter, so a used one is refused naming it rather
    /// than approximated.
    RemoveUnusedParam,
    /// rust-analyzer `convert_tuple_return_type_to_struct`: `-> (A, B)` becomes `-> Named`, a new
    /// tuple struct, and every destructuring caller is rewritten to match. Anchored on the function;
    /// `name` is the new struct's, given to the assist's placeholder through the server's rename.
    ConvertTupleReturnToStruct,
    /// Changes one parameter's type in the declaration, and nothing else: every caller is its own
    /// call-site operation, normally in the same transactional group. Anchored on the function;
    /// `name` is the parameter, `type` the new type.
    ChangeParamType,
    /// Adds a parameter to the declaration only. Anchored on the function; `name` and `type` are the
    /// new parameter's, `variant` its position — `first`, `last` or `after:<param>`.
    AddParam,
    /// Reorders the declaration's parameters only. Anchored on the function; `order` names every
    /// parameter once, in the new order.
    ReorderParams,
    /// Changes the declaration's return type. Anchored on the function; either `type`, the new
    /// return type written into the declaration (the body and callers are left to the group's gate),
    /// or `variant` = `wrap_result` / `wrap_option` / `unwrap`, backed by rust-analyzer's
    /// `wrap_return_type` / `unwrap_return_type` assists, which also rewrite the function's own
    /// returns.
    ChangeReturnType,
    /// Inserts one argument into one call expression. Anchored on the caller with a relative range
    /// on the call; `expr` is the argument, `variant` its position — `first`, `last` or a one-based
    /// index.
    AddCallArg,
    /// Removes one argument from one call expression; `variant` is its position.
    RemoveCallArg,
    /// Replaces one argument of one call expression; `variant` is its position, `expr` the new
    /// argument.
    ChangeCallArg,
    /// Reorders the arguments of one call expression; `order` lists the current one-based argument
    /// positions in their new order.
    ReorderCallArgs,
    /// Moves members of an inherent `impl` to another type of the same crate: the whole block's
    /// self type changes, or the block is split at the anchored run. `to_type` names the new type.
    RetargetImpl,
    /// Re-points the callee of one call (anchored on the call, `callee` the complete new callee),
    /// or the receiver of every call of one method (anchored on the method, `callee` a
    /// `$receiver<hops>.<method>` template). The arguments are kept byte for byte.
    RepointCall,
}

impl RefactorKind {
    /// Whether this operation edits one call expression rather than a declaration — the four whose
    /// anchor must land on a call.
    #[must_use]
    pub fn edits_a_call_site(self) -> bool {
        matches!(
            self,
            RefactorKind::AddCallArg
                | RefactorKind::RemoveCallArg
                | RefactorKind::ChangeCallArg
                | RefactorKind::ReorderCallArgs
        )
    }

    /// Whether this operation moves modules out of the crate that holds them.
    ///
    /// The two cross-crate moves differ only in how many modules travel, so every decision taken
    /// about one — the destination it must name, the facade it may leave, the preconditions read
    /// before a server is spawned — is taken about both.
    ///
    /// [`RefactorKind::MoveTestBinaryToCrate`] crosses a crate boundary too and is still **not**
    /// one of these, because this predicate does not mean "crosses a boundary" — it means "moves a
    /// *module*", and every caller reads it that way. Each of the three refusals it gates is
    /// already made for a test binary, earlier and in words about a test binary; and the fourth
    /// caller, [`unrunnable_moves`](crate::unrunnable_moves), reads each anchor as
    /// `<crate>/src/<module>.rs` to find the `mod` line it is about. A test binary has no `mod`
    /// line anywhere and does not live under `src/`, so admitting it here would report every
    /// well-formed test-binary move as an operation that cannot run.
    ///
    /// TODO(carve-test-homes): give `check` a static preflight for a test-binary move of its own —
    /// the anchor's shape and both crates' manifests are all readable before a server is spawned,
    /// so `apply`'s refusals are reachable statically the way a module move's are.
    #[must_use]
    pub fn moves_across_crates(self) -> bool {
        matches!(
            self,
            RefactorKind::MoveModuleToCrate | RefactorKind::MoveClusterToCrate
        )
    }
}

//! The Rust backend's operations are free functions over a session, and the type keeps only its
//! construction surface, its session and its trait impls (`#reshape` 18/19).
//!
//! A type's inherent impls must be written in its defining crate (E0116), so every operation that
//! is an `impl RustBackend` block pins its module to the type's crate. After `#reshape` 18, inherent
//! blocks sit only in the type's file (the constructors) and in the session modules, and every
//! operation's resolver takes `session: &mut RustBackend` first. A text check over `src/`: comment
//! lines are dropped first, so a doc comment that mentions a name is not code.

use std::path::PathBuf;

/// The type's file, and the session modules — the same list as `SESSION_MODULES` in
/// `tests/engine_module_edges_shape.rs` (`handshake` and `lsp_edits` hold no block at all).
const FILES_THAT_MAY_HOLD_AN_INHERENT_BLOCK: [&str; 8] = [
    "src/backends/rust.rs",
    "src/backends/rust/handshake.rs",
    "src/backends/rust/transport.rs",
    "src/backends/rust/readiness.rs",
    "src/backends/rust/documents.rs",
    "src/backends/rust/references.rs",
    "src/backends/rust/lsp_edits.rs",
    "src/backends/rust/projection.rs",
];

/// The construction surface `rust.rs`'s one inherent block keeps.
const THE_CONSTRUCTION_SURFACE: [&str; 8] = [
    "new",
    "with_progress",
    "with_cancellation",
    "with_spawn_recorder",
    "with_trace",
    "with_wait_heartbeat",
    "with_silence_bounds",
    "from_lsp_client",
];

/// Every operation the Rust backend carried as a method before `#reshape` 18, which must now be a
/// free function — so no inherent member of the type may carry one of these names.
const THE_OPERATION_INVENTORY: [&str; 52] = [
    // the dispatcher and the assist offer
    "resolve_opening",
    "outside_references_opening",
    "inference_ready_at",
    "assist",
    "offered_assist",
    // assist results, extraction, the seam survey, symbols
    "edit_for",
    "chain_module_to_file",
    "multi_file_assist",
    "assisted_edit",
    "extract",
    "rename_placeholder",
    "prune_assist_imports",
    "unresolved_names",
    "cleaned_extraction",
    "verified_narrowings",
    "respelled_signature_types",
    "survey_moved_items",
    "survey_impl_members",
    "reach_of",
    "anchor_range",
    "locate_symbol",
    "rename_symbol",
    // signature operations
    "rewrite_signature",
    "wrap_or_unwrap_return_type",
    "name_converted_struct",
    "rename_introduced",
    // imports
    "restore_imports",
    "next_import",
    "offered_imports",
    "first_offer_that_resolves",
    "reconstructed",
    "unresolved_after",
    "unresolved_the_seam_lost",
    // item paths
    "outline_of",
    "resolve_item_opening",
    "item_enclosing_opening",
    // same-crate moves
    "same_crate_move",
    "move_items",
    "range_of",
    "sites_of",
    "move_reach",
    "reparent_module",
    "callers_of_the_module",
    "move_impl_members",
    // the remaining operations
    "retarget_impl",
    "retarget_range",
    "repoint_call",
    "repoint_facade_imports",
    "read_fields_through",
    "detach_method",
    "reached_by_the_moved_code",
    "reached_by_the_tree",
];

/// Each operation's resolver: the module that holds it and its name.
const THE_OPERATION_RESOLVERS: [(&str, &str); 10] = [
    ("src/backends/rust/language_backend.rs", "resolve_opening"),
    ("src/backends/rust/item_move.rs", "move_items"),
    ("src/backends/rust/module_reparent.rs", "reparent_module"),
    ("src/backends/rust/retarget_impl.rs", "retarget_impl"),
    ("src/backends/rust/repoint_call.rs", "repoint_call"),
    (
        "src/backends/rust/repoint_facade.rs",
        "repoint_facade_imports",
    ),
    ("src/backends/rust/signature.rs", "name_converted_struct"),
    (
        "src/backends/rust/read_fields_through.rs",
        "read_fields_through",
    ),
    ("src/backends/rust/impl_move.rs", "move_impl_members"),
    ("src/backends/rust/detach_method.rs", "detach_method"),
];

fn the_code_of(relative: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{relative} should exist: {error}"));
    text.lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Every `.rs` file under `relative`, as paths relative to the package.
fn rust_files_below(relative: &str) -> Vec<String> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut pending = vec![root.join(relative)];
    let mut found = Vec::new();
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(&dir).expect("a readable source directory") {
            let path = entry.expect("a directory entry").path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                let below = path.strip_prefix(&root).expect("under the package");
                found.push(below.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    found.sort();
    found
}

/// The production code of `relative`: everything before its first inline `#[cfg(test)]` module.
fn the_production_code_of(relative: &str) -> String {
    let code = the_code_of(relative);
    match code.find("#[cfg(test)]\nmod ") {
        Some(at) => code[..at].to_string(),
        None => code,
    }
}

/// The names of the members of every inherent `impl RustBackend {` block of `relative`.
fn inherent_members_of(relative: &str) -> Vec<String> {
    let code = the_production_code_of(relative);
    let mut members = Vec::new();
    let mut inside = false;
    for line in code.lines() {
        if line == "impl RustBackend {" {
            inside = true;
        } else if inside && line == "}" {
            inside = false;
        } else if inside {
            if let Some(name) = member_declared_on(line) {
                members.push(name);
            }
        }
    }
    members
}

/// The function a member line at the block's indentation declares, if it declares one.
fn member_declared_on(line: &str) -> Option<String> {
    let rest = line.strip_prefix("    ")?;
    if rest.starts_with(' ') {
        return None;
    }
    let after_fn = rest.split("fn ").nth(1)?;
    let name: String = after_fn
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    (!name.is_empty()).then_some(name)
}

#[test]
fn an_inherent_impl_of_rust_backend_sits_only_in_the_type_file_and_the_session_modules() {
    // Given every module of the Rust backend
    let files = rust_files_below("src/backends/rust");

    // When the files holding an inherent `impl RustBackend` block are listed
    let holding: Vec<&String> = files
        .iter()
        .filter(|file| !FILES_THAT_MAY_HOLD_AN_INHERENT_BLOCK.contains(&file.as_str()))
        .filter(|file| {
            the_production_code_of(file)
                .lines()
                .any(|line| line == "impl RustBackend {")
        })
        .collect();

    // Then there are none outside the type's file and the session modules
    assert!(
        holding.is_empty(),
        "these modules still hold an inherent block of the type: {holding:?}"
    );
}

#[test]
fn the_type_file_keeps_only_the_construction_surface() {
    // Given the type's own file
    // When the members of its inherent block are listed
    let members = inherent_members_of("src/backends/rust.rs");

    // Then each is a constructor or a builder
    let beyond: Vec<&String> = members
        .iter()
        .filter(|member| !THE_CONSTRUCTION_SURFACE.contains(&member.as_str()))
        .collect();
    assert!(
        beyond.is_empty(),
        "rust.rs keeps more than construction: {beyond:?}"
    );
}

#[test]
fn every_operation_resolver_is_a_function_whose_first_parameter_is_the_session() {
    // Given each operation's resolver and the module that holds it
    // When each is looked for as a free function taking the session first
    let not_free: Vec<String> = THE_OPERATION_RESOLVERS
        .iter()
        .filter(|(file, name)| {
            let code =
                std::fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(file))
                    .unwrap_or_default();
            let flat: String = code.split_whitespace().collect::<Vec<_>>().join(" ");
            !flat.contains(&format!("fn {name}( session: &mut RustBackend,"))
                && !flat.contains(&format!("fn {name}(session: &mut RustBackend,"))
        })
        .map(|(file, name)| format!("{file}: {name}"))
        .collect();

    // Then each is one
    assert!(
        not_free.is_empty(),
        "these resolvers are not functions over `session: &mut RustBackend`: {not_free:?}"
    );
}

#[test]
fn no_method_of_rust_backend_carries_a_name_of_the_operation_inventory() {
    // Given every inherent member of the type, wherever its block is
    let members: Vec<(String, String)> = rust_files_below("src/backends")
        .into_iter()
        .flat_map(|file| {
            inherent_members_of(&file)
                .into_iter()
                .map(move |member| (file.clone(), member))
        })
        .collect();

    // When those named in the operation inventory are listed
    let forwarders: Vec<&(String, String)> = members
        .iter()
        .filter(|(_, member)| THE_OPERATION_INVENTORY.contains(&member.as_str()))
        .collect();

    // Then there are none: no method stands in for, or forwards to, a detached function
    assert!(
        forwarders.is_empty(),
        "these operations are still methods of the type: {forwarders:?}"
    );
}

#[test]
fn the_runner_reaches_the_backend_only_through_construction_and_the_shared_sinks() {
    // Given every file of the runner
    let allowed = [
        "RustBackend",
        "ProgressSink",
        "discard",
        "WAIT_HEARTBEAT",
        "human_delta",
    ];

    // When the items it names from `backends::rust` are listed
    let named: Vec<String> = rust_files_below("src/runner")
        .into_iter()
        .chain(["src/runner.rs".to_string()])
        .flat_map(|file| {
            let code = the_code_of(&file);
            let mut found = Vec::new();
            for (at, _) in code.match_indices("backends::rust::") {
                let rest = &code[at + "backends::rust::".len()..];
                let names = match rest.strip_prefix('{') {
                    Some(group) => group[..group.find('}').unwrap_or(group.len())].to_string(),
                    None => rest
                        .chars()
                        .take_while(|c| c.is_alphanumeric() || *c == '_')
                        .collect(),
                };
                found.extend(
                    names
                        .split(',')
                        .map(|name| name.trim().to_string())
                        .filter(|name| !name.is_empty()),
                );
            }
            found
        })
        .filter(|name| !allowed.contains(&name.as_str()))
        .collect();

    // Then it names nothing else: the runner constructs the backend and dispatches through the trait
    assert!(
        named.is_empty(),
        "the runner reaches into the backend: {named:?}"
    );
}

#[test]
fn the_five_trait_impls_stay_on_rust_backend_once_each() {
    // Given every module of the Rust backend
    let headers: Vec<String> = rust_files_below("src/backends")
        .into_iter()
        .flat_map(|file| {
            the_production_code_of(&file)
                .lines()
                .filter(|line| line.starts_with("impl ") && line.contains(" for RustBackend"))
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .collect();

    // When each trait impl's header is counted
    let counted: Vec<(&str, usize)> = [
        "impl Drop for RustBackend {",
        "impl LanguageBackend for RustBackend {",
        "impl ModuleReferences for RustBackend {",
        "impl ItemResolver for RustBackend {",
        "impl ItemAtResolver for RustBackend {",
    ]
    .into_iter()
    .map(|header| {
        (
            header,
            headers.iter().filter(|line| *line == header).count(),
        )
    })
    .collect();

    // Then each is there exactly once: dispatch stays through the trait object
    assert!(
        counted.iter().all(|(_, count)| *count == 1),
        "{counted:?} among {headers:?}"
    );
}

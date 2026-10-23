//! Acceptance: an installed Zed extension's languages get anchors and outlines from their grammar
//! (`#reshape` extension-outline, PRD acceptance criteria; changeset R1–R8).
//!
//! Every test loads the real vendored tree-sitter-toml grammar (`tests/fixtures/grammars/`) through
//! tree-sitter's WASM loader, with no language server. Item anchors go through the same entry
//! points Rust's do — `items_anchor` (`anchors --items`), `item_anchor_at` (`anchors --at`),
//! `span_of` / `resolve_item_anchors` (a plan's anchors) and the static `check` — routed by the
//! `BackendRegistry` to the extension language's `ItemResolver` / `ItemAtResolver`.

mod outline_support;

use std::path::Path;
use std::sync::Arc;

use outline_support::{
    a_backend_registry_of, a_workspace_holding_the_app_config, an_extensions_dir_with, at, span,
    the_fingerprint_of_lines, AFixtureExtension, APP, APP_TOML,
};
use pretty_assertions::assert_eq;
use tddy_code_restructuring::item_anchor::{item_anchor_at, items_anchor, span_of};
use tddy_code_restructuring::runner::{check_plan, Command, Options};
use tddy_code_restructuring::{
    Anchor, ItemPath, LanguageSupport, Plan, RefactorKind, RestructureError, SupportRegistry,
};
use tddy_zed_extensions::{load_installed, InstalledExtensions, OutlineEntry};
use tokio_util::sync::CancellationToken;

/// `config/app.toml::<segments>`.
fn an_app_item(segments: &str) -> ItemPath {
    ItemPath::parse(&format!("{APP}::{segments}")).expect("the item path parses")
}

fn names(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| (*item).to_string()).collect()
}

fn an_entry(
    path: &[&str],
    name: &str,
    span: tddy_code_restructuring::Range,
    name_at: tddy_code_restructuring::Position,
) -> OutlineEntry {
    OutlineEntry {
        path: names(path),
        name: name.to_string(),
        span,
        name_at,
    }
}

#[test]
fn an_extension_language_claims_its_suffixes() {
    // Given the demo-toml extension installed
    let installed = an_extensions_dir_with(&[AFixtureExtension::DemoToml]);

    // When its languages are loaded
    let languages = load_installed(&installed.extensions).expect("the extensions load");

    // Then its one language is a support named after it (R1) that claims its `path_suffixes` and
    // nothing else (R2)
    let support = &languages.supports[0];
    assert_eq!(
        (
            languages.supports.len(),
            support.id().to_string(),
            support.claims(Path::new(APP)),
            support.claims(Path::new("docs/README.md")),
        ),
        (1, "demo-toml".to_string(), true, false)
    );
}

#[test]
fn a_builtin_keeps_a_suffix_an_extension_also_claims() {
    // Given the built-ins, then the demo-toml extension, which also claims `rs`
    let installed = an_extensions_dir_with(&[AFixtureExtension::DemoToml]);
    let mut supports = SupportRegistry::with_builtins();
    let failures = supports
        .with_installed_extensions(&installed.extensions)
        .expect("the extensions register");

    // When a Rust file and a TOML file are routed
    let language_of = |path: &str| {
        supports
            .support_of(Path::new(path))
            .map(|support| support.id().to_string())
    };
    let routed = (language_of("src/lib.rs"), language_of(APP));

    // Then the built-in keeps `.rs` and the extension takes `.toml`, with nothing to report
    assert_eq!(
        (routed, failures.len()),
        ((Some("rust".to_string()), Some("demo-toml".to_string())), 0)
    );
}

#[test]
fn the_outline_lists_each_item_with_its_context() {
    // Given the demo-toml language
    let installed = an_extensions_dir_with(&[AFixtureExtension::DemoToml]);
    let languages = load_installed(&installed.extensions).expect("the extensions load");

    // When the app config is outlined
    let outline = languages.supports[0].outline_of(APP_TOML);

    // Then every `@item` is an entry: a pair carries its table as context, a table's span ends at
    // its last pair rather than running on over the blank line to the next table (R3)
    assert_eq!(
        outline.entries,
        vec![
            an_entry(&[], "server", span(at(1, 1), at(3, 12)), at(1, 2)),
            an_entry(&["server"], "host", span(at(2, 1), at(2, 19)), at(2, 1)),
            an_entry(&["server"], "port", span(at(3, 1), at(3, 12)), at(3, 1)),
            an_entry(&[], "database", span(at(5, 1), at(7, 9)), at(5, 2)),
            an_entry(&["database"], "url", span(at(6, 1), at(6, 33)), at(6, 1)),
            an_entry(&["database"], "pool", span(at(7, 1), at(7, 9)), at(7, 1)),
            an_entry(&[], "logging", span(at(9, 1), at(10, 15)), at(9, 2)),
            an_entry(
                &["logging"],
                "level",
                span(at(10, 1), at(10, 15)),
                at(10, 1)
            ),
        ]
    );
}

#[test]
fn anchors_print_an_item_path_and_fingerprint_per_entry() {
    // Given a workspace with the app config, and the demo-toml language built over it
    let installed = an_extensions_dir_with(&[AFixtureExtension::DemoToml]);
    let workspace = a_workspace_holding_the_app_config();
    let languages = load_installed(&installed.extensions).expect("the extensions load");
    let mut registry = a_backend_registry_of(&languages, &workspace);

    // When `anchors --items server` is asked of the file
    let anchor = items_anchor(workspace.root(), APP, &names(&["server"]), &mut registry)
        .expect("the anchor is emitted");

    // Then it names the entry by `<file>::<name>` and carries the fingerprint of its whole lines,
    // as Rust's anchors do (R4, R5)
    assert_eq!(
        anchor,
        Anchor::Items {
            file: APP.to_string(),
            items: vec![an_app_item("server")],
            fingerprints: vec![the_fingerprint_of_lines(1, 3)],
        }
    );
}

#[test]
fn an_at_anchor_names_the_innermost_entry_with_its_context() {
    // Given a workspace with the app config, and the demo-toml language built over it
    let installed = an_extensions_dir_with(&[AFixtureExtension::DemoToml]);
    let workspace = a_workspace_holding_the_app_config();
    let languages = load_installed(&installed.extensions).expect("the extensions load");
    let mut registry = a_backend_registry_of(&languages, &workspace);

    // When `anchors --at 3:3` is asked of the file — inside `port`, inside `[server]`
    let anchor = item_anchor_at(
        workspace.root(),
        APP,
        span(at(3, 3), at(3, 3)),
        &mut registry,
    )
    .expect("the anchor is emitted");

    // Then the innermost entry is named through its context, `<file>::server::port` (R4)
    assert_eq!(
        anchor,
        Anchor::Item {
            item: an_app_item("server::port"),
            file: APP.to_string(),
            start: Some(at(1, 3)),
            end: Some(at(1, 3)),
            fingerprint: the_fingerprint_of_lines(3, 3),
            hint: Some(at(3, 3)),
        }
    );
}

#[test]
fn an_items_anchor_over_adjacent_entries_resolves() {
    // Given a plan line anchored on two adjacent tables, `server` then `database`
    let installed = an_extensions_dir_with(&[AFixtureExtension::DemoToml]);
    let workspace = a_workspace_holding_the_app_config();
    let languages = load_installed(&installed.extensions).expect("the extensions load");
    let mut registry = a_backend_registry_of(&languages, &workspace);
    let anchor = Anchor::Items {
        file: APP.to_string(),
        items: vec![an_app_item("server"), an_app_item("database")],
        fingerprints: vec![
            the_fingerprint_of_lines(1, 3),
            the_fingerprint_of_lines(5, 7),
        ],
    };

    // When the anchor is resolved the way a run resolves it
    let covered = span_of(&anchor, workspace.root(), &mut registry).expect("the anchor resolves");

    // Then it covers both tables, the blank line between them included (R5)
    assert_eq!(covered, span(at(1, 1), at(7, 9)));
}

#[test]
fn non_adjacent_entries_are_refused() {
    // Given the demo-toml language built over a workspace with the app config
    let installed = an_extensions_dir_with(&[AFixtureExtension::DemoToml]);
    let workspace = a_workspace_holding_the_app_config();
    let languages = load_installed(&installed.extensions).expect("the extensions load");
    let mut registry = a_backend_registry_of(&languages, &workspace);

    // When `anchors --items server,logging` names two tables with `[database]` between them
    let refused = items_anchor(
        workspace.root(),
        APP,
        &names(&["server", "logging"]),
        &mut registry,
    );

    // Then it is refused in Rust's words (R5)
    match refused {
        Err(RestructureError::SeamRefused(reason)) => assert_eq!(
            reason,
            format!(
                "the named items are not adjacent: `{APP}::server` and `{APP}::logging` have \
                 other lines between them, and a seam is one contiguous range — a span reaching \
                 from one to the other would carry everything in between."
            )
        ),
        other => panic!("expected the adjacency refusal, got {other:?}"),
    }
}

#[test]
fn check_reports_a_stale_fingerprint() {
    // Given a plan whose `items` anchor on `server` was taken before `port` changed
    let installed = an_extensions_dir_with(&[AFixtureExtension::DemoToml]);
    let workspace = a_workspace_holding_the_app_config();
    let fingerprint_before =
        serde_json::to_string(&the_fingerprint_of_lines(1, 3)).expect("a fingerprint is JSON");
    workspace.rewrite_the_app_config(&APP_TOML.replace("port = 8080", "port = 9090"));
    let digest = tddy_code_restructuring::apply::hash_file(&workspace.root().join(APP))
        .expect("hash the app config");
    let plan = Plan::parse(&format!(
        "{{\"v\":1,\"snapshot\":{{\"{APP}\":\"{digest}\"}}}}\n\
         {{\"op\":\"extract_module\",\"anchor\":{{\"kind\":\"items\",\"file\":\"{APP}\",\
         \"items\":[\"{APP}::server\"],\"fingerprints\":[{fingerprint_before}]}},\
         \"name\":\"moved\"}}\n"
    ))
    .expect("the plan parses");
    let mut supports = SupportRegistry::new();
    supports
        .with_installed_extensions(&installed.extensions)
        .expect("the extensions register");

    // When a static check is run over it, with the installed languages and no server
    let checked = check_plan(
        workspace.root(),
        plan,
        Options {
            command: Command::Check,
            installed: Arc::new(supports),
            ..Options::default()
        },
        None,
        CancellationToken::new(),
    );

    // Then the stale item is refused as Rust's is, naming it and its file (R6)
    match checked {
        Err(RestructureError::ItemChanged { item, file }) => {
            assert_eq!((item, file), (format!("{APP}::server"), APP.to_string()))
        }
        other => panic!("expected the item to be refused as changed, got {other:?}"),
    }
}

#[test]
fn a_rename_is_unsupported_without_a_server() {
    // Given the demo-toml language, which runs no server
    let installed = an_extensions_dir_with(&[AFixtureExtension::DemoToml]);
    let workspace = a_workspace_holding_the_app_config();
    let languages = load_installed(&installed.extensions).expect("the extensions load");
    let mut registry = a_backend_registry_of(&languages, &workspace);

    // When a rename is routed to a file of it
    let routed = registry
        .backend_for(Path::new(APP), RefactorKind::RenameSymbol)
        .map(|backend| backend.language().to_string());

    // Then it is refused as unsupported, naming the language, and the support offers no server to
    // start (R7)
    match routed {
        Err(RestructureError::UnsupportedOp { backend, op }) => assert_eq!(
            (
                backend.as_str(),
                op.as_str(),
                languages.supports[0].launch()
            ),
            ("demo-toml", "RenameSymbol", None)
        ),
        other => panic!("expected UnsupportedOp, got {other:?}"),
    }
}

#[test]
fn a_broken_grammar_is_reported_once_and_spares_other_languages() {
    // Given demo-toml and an extension whose two languages share a grammar that is not WebAssembly
    let installed = an_extensions_dir_with(&[
        AFixtureExtension::DemoToml,
        AFixtureExtension::BrokenGrammar,
    ]);

    // When the installed languages are loaded
    let languages = load_installed(&installed.extensions).expect("one broken grammar fails no run");

    // Then the grammar is reported once, naming its extension and itself, and only demo-toml's
    // language is a support (R8)
    let reported: Vec<(String, String)> = languages
        .failures
        .iter()
        .map(|failure| (failure.extension.clone(), failure.grammar.clone()))
        .collect();
    let loaded: Vec<String> = languages
        .supports
        .iter()
        .map(|support| support.id().to_string())
        .collect();
    assert_eq!(
        (reported, loaded),
        (
            vec![("broken-grammar".to_string(), "broken".to_string())],
            vec!["demo-toml".to_string()]
        )
    );
}

#[test]
fn a_grammar_load_failure_says_which_extension_and_grammar() {
    // Given an extension whose grammar is not WebAssembly
    let installed = an_extensions_dir_with(&[AFixtureExtension::BrokenGrammar]);

    // When the installed languages are loaded
    let languages = load_installed(&installed.extensions).expect("one broken grammar fails no run");

    // Then the one report reads as the PRD words it (R8)
    let report = languages.failures[0].to_string();
    assert!(
        report.starts_with("extension broken-grammar: grammar broken did not load: "),
        "unexpected report: {report}"
    );
}

#[test]
fn a_snapshot_re_resolves_an_extension_languages_item_anchors_without_a_server() {
    // Given a plan file whose only item anchor lies in a file the server-less demo-toml claims
    let installed = an_extensions_dir_with(&[AFixtureExtension::DemoToml]);
    let workspace = a_workspace_holding_the_app_config();
    let fingerprint =
        serde_json::to_string(&the_fingerprint_of_lines(1, 3)).expect("a fingerprint is JSON");
    let plan = workspace.root().join("plan.jsonl");
    std::fs::write(
        &plan,
        format!(
            "{{\"v\":1,\"snapshot\":{{}}}}\n\
             {{\"op\":\"extract_module\",\"anchor\":{{\"kind\":\"items\",\"file\":\"{APP}\",\
             \"items\":[\"{APP}::server\"],\"fingerprints\":[{fingerprint}]}},\"name\":\"moved\"}}\n"
        ),
    )
    .expect("write the plan");
    let mut supports = SupportRegistry::new();
    supports
        .with_installed_extensions(&installed.extensions)
        .expect("the extensions register");

    // When its snapshot is rewritten with no language server
    let rewritten = tddy_code_restructuring::runner::snapshot_resolving(
        workspace.root(),
        Options {
            command: Command::Snapshot,
            target: Some(plan),
            installed: Arc::new(supports),
            ..Options::default()
        },
        None,
        CancellationToken::new(),
    );

    // Then the anchor is re-resolved from the file's text, and nothing is stale
    let rewrite = rewritten.expect("a server-less item anchor needs no server");
    assert_eq!(rewrite.stale.len(), 0);
}

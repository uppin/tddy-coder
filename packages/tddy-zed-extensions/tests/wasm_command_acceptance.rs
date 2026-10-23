//! Acceptance: an opted-in extension's WASM says how to start its language server (`#reshape`
//! extension-wasm-command, PRD acceptance criteria; changeset R1–R5).
//!
//! The extension is `wasm-fixture` (`tests/fixtures/wasm-ext`): a real Zed code extension built
//! for `wasm32-wasip2` with `zed_extension_api` 0.6.0 from its committed source. Its
//! `language_server_command` answers per server id — `fake-ls` from `worktree.which`,
//! `downloading-ls` through `download_file`, `looping-ls` never. The server it names is
//! `tddy-lsp`'s deterministic `fake_lsp`, reached through the worktree's own `PATH`.

mod wasm_support;

use std::time::{Duration, Instant};

use pretty_assertions::assert_eq;
use serde_json::json;
use tddy_lsp::LaunchSpec;
use tddy_zed_extensions::server::client_capabilities;
use tddy_zed_extensions::wasm::download_refusal;
use tddy_zed_extensions::{
    choose_server, install_from_path, set_wasm_enabled, ExtensionError, ExtensionSettings,
    ExtensionsDir, Index, InstallOutcome, ServerChoice, WasmCommandError, WasmHost,
};
use wasm_support::{
    a_rust_lib_built_for, demo_toml_of, the_fixture_source_at_version, the_index_entry,
    the_server_started_by, AWorktree, AnInstalledWasmExtension, DOWNLOADING_LS, FAKE_LS,
    LOOPING_LS, WASM_FIXTURE,
};

/// A deadline short enough that the looping server does not hold the suite up.
const A_SHORT_DEADLINE: Duration = Duration::from_millis(300);

/// What `choose_server` makes of K22's `choice` for Demo TOML, with the fixture opted in and
/// loaded through `host`.
fn the_choice_through_the_wasm(
    host: &WasmHost,
    installed: &AnInstalledWasmExtension,
    worktree: &AWorktree,
    choice: ServerChoice,
) -> Result<ServerChoice, WasmCommandError> {
    let mut wasm = host
        .load_opted_in(
            &the_index_entry(true),
            &installed.extension_dir(),
            &a_rust_lib_built_for("0.6.0"),
            &ExtensionSettings::default(),
        )
        .expect("the opted-in fixture loads")
        .expect("an opted-in extension with an extension.wasm is loaded");
    choose_server(
        &installed.manifest(),
        &demo_toml_of(installed),
        choice,
        Some(&mut wasm),
        &worktree.as_wasm_worktree(),
    )
}

#[tokio::test(flavor = "multi_thread")]
async fn an_opted_in_extension_starts_the_server_its_wasm_names() {
    // Given the fixture installed and opted in, fake-ls declared with no binary path, and a
    // worktree whose PATH holds fake-ls
    let installed = AnInstalledWasmExtension::built_from_its_source();
    let worktree = AWorktree::with_fake_ls_on_its_path();
    let host = WasmHost::new().expect("a host");

    // When the language's server is chosen
    let chosen = the_choice_through_the_wasm(
        &host,
        &installed,
        &worktree,
        installed.unconfigured(FAKE_LS),
    )
    .expect("the WASM names a command");

    // Then the launch is the WASM's command, options and configuration, with tddy's capabilities
    let mut launch = LaunchSpec::new(worktree.fake_ls())
        .with_capabilities(client_capabilities())
        .with_initialization_options(json!({ "startedBy": "wasm", "server": FAKE_LS }))
        .with_workspace_configuration(json!({ "fixture": { "mode": "wasm" } }));
    launch.args = vec!["--stdio".to_string()];
    launch.env = vec![("FIXTURE_STARTED_BY".to_string(), "wasm".to_string())];
    assert_eq!(
        chosen,
        ServerChoice::Configured {
            server: FAKE_LS.to_string(),
            launch: launch.clone(),
        }
    );

    // And that launch starts the server
    let started = the_server_started_by(launch, worktree.root()).await;
    assert!(
        started.is_ok(),
        "the WASM's server did not start: {started:?}"
    );
}

#[test]
fn an_extension_not_opted_in_never_loads_its_wasm() {
    // Given the fixture installed with an extension.wasm no compiler would accept
    let installed = AnInstalledWasmExtension::with_an_unloadable_wasm();
    let host = WasmHost::new().expect("a host");
    let load_with = |wasm_enabled: bool| {
        host.load_opted_in(
            &the_index_entry(wasm_enabled),
            &installed.extension_dir(),
            &a_rust_lib_built_for("0.6.0"),
            &ExtensionSettings::default(),
        )
        .map(|loaded| loaded.is_some())
    };

    // When it is loaded not opted in, and — for contrast — opted in
    let not_opted_in = load_with(false);
    let opted_in = load_with(true).map_err(|error| matches!(error, WasmCommandError::Load { .. }));

    // Then not opted in nothing is read (R1); opted in the same file is refused
    assert_eq!((not_opted_in, opted_in), (Ok(false), Err(true)));
}

#[test]
fn a_configured_binary_path_wins_over_the_wasm() {
    // Given the fixture opted in, and looping-ls — whose WASM never returns — configured with a
    // binary path
    let installed = AnInstalledWasmExtension::built_from_its_source();
    let worktree = AWorktree::with_fake_ls_on_its_path();
    let host = WasmHost::new()
        .expect("a host")
        .with_deadline(A_SHORT_DEADLINE);
    let configured = ServerChoice::Configured {
        server: LOOPING_LS.to_string(),
        launch: LaunchSpec::new("/opt/looping-ls"),
    };

    // When the language's server is chosen
    let chosen = the_choice_through_the_wasm(&host, &installed, &worktree, configured.clone());

    // Then the configured binary is kept, and the WASM was never asked (R2)
    assert_eq!(chosen, Ok(configured));
}

#[test]
fn a_download_call_refuses_the_run_with_the_install_hint() {
    // Given the fixture opted in, and downloading-ls — whose WASM calls download_file — declared
    // with no binary path
    let installed = AnInstalledWasmExtension::built_from_its_source();
    let worktree = AWorktree::with_fake_ls_on_its_path();
    let host = WasmHost::new().expect("a host");

    // When the language's server is chosen
    let refused = the_choice_through_the_wasm(
        &host,
        &installed,
        &worktree,
        installed.unconfigured(DOWNLOADING_LS),
    );

    // Then the download was refused with the install hint, and the command fails carrying it (R4)
    assert_eq!(
        refused.map_err(|error| error.to_string()),
        Err(format!(
            "extension {WASM_FIXTURE}: language server {DOWNLOADING_LS}: {}",
            download_refusal(DOWNLOADING_LS)
        ))
    );
}

#[test]
fn an_unsupported_api_version_is_refused_with_the_range() {
    // Given the fixture's manifest claiming zed_extension_api 0.5.0, and no extension.wasm at all
    let installed = AnInstalledWasmExtension::without_its_wasm();
    let host = WasmHost::new().expect("a host");

    // When it is loaded
    let refused = host
        .load(
            WASM_FIXTURE,
            &installed.extension_dir(),
            &a_rust_lib_built_for("0.5.0"),
            &ExtensionSettings::default(),
        )
        .map(|loaded| loaded.extension().to_string());

    // Then it is refused by version, with the supported range, before the file is looked for (R3)
    assert_eq!(
        refused.map_err(|error| error.to_string()),
        Err(format!(
            "extension {WASM_FIXTURE}: built for zed_extension_api 0.5.0; tddy supports 0.6.0..<0.7.0"
        ))
    );
}

#[test]
fn a_looping_call_is_stopped_after_the_deadline() {
    // Given the fixture opted in, looping-ls — whose WASM never returns — declared with no binary
    // path, and a host with a short deadline
    let installed = AnInstalledWasmExtension::built_from_its_source();
    let worktree = AWorktree::with_fake_ls_on_its_path();
    let host = WasmHost::new()
        .expect("a host")
        .with_deadline(A_SHORT_DEADLINE);

    // When the language's server is chosen
    let started = Instant::now();
    let stopped = the_choice_through_the_wasm(
        &host,
        &installed,
        &worktree,
        installed.unconfigured(LOOPING_LS),
    );
    let took = started.elapsed();

    // Then the call is stopped and reported, long before anything else would end it (R5)
    assert_eq!(
        (stopped, took < Duration::from_secs(5)),
        (
            Err(WasmCommandError::DeadlineExceeded {
                extension: WASM_FIXTURE.to_string(),
                server: LOOPING_LS.to_string(),
                call: "language-server-command".to_string(),
                deadline: A_SHORT_DEADLINE,
            }),
            true
        )
    );
}

#[test]
fn enable_and_disable_wasm_flip_the_index_entry() {
    // Given the fixture installed and recorded in the index, not opted in
    let installed = AnInstalledWasmExtension::built_from_its_source();
    installed.write_index_with_wasm_enabled(false);
    let recorded = || {
        Index::load(&installed.extensions)
            .expect("the index loads")
            .extensions
            .get(WASM_FIXTURE)
            .map(|entry| entry.wasm_enabled)
    };

    // When it is opted in, then out, and an extension that is not installed is opted in
    let enabled =
        set_wasm_enabled(&installed.extensions, WASM_FIXTURE, true).map(|entry| entry.wasm_enabled);
    let after_enable = recorded();
    let disabled = set_wasm_enabled(&installed.extensions, WASM_FIXTURE, false)
        .map(|entry| entry.wasm_enabled);
    let after_disable = recorded();
    let unknown = set_wasm_enabled(&installed.extensions, "not-installed", true);

    // Then the entry follows each command, and the unknown id is refused by name
    assert_eq!(
        (enabled, after_enable, disabled, after_disable, unknown),
        (
            Ok(true),
            Some(true),
            Ok(false),
            Some(false),
            Err(ExtensionError::NotInstalled {
                extension: "not-installed".to_string()
            })
        )
    );
}

#[test]
fn a_new_version_of_an_opted_in_extension_stays_opted_in() {
    // Given the fixture installed from its source and opted in
    let data = tempfile::TempDir::new().expect("temp data dir");
    let extensions = ExtensionsDir::under_data_dir(data.path());
    let first = the_fixture_source_at_version("0.1.0");
    let fresh = install_from_path(&extensions, first.path()).map(|outcome| match outcome {
        InstallOutcome::Installed(entry) => entry.wasm_enabled,
        other => panic!("a first install is Installed, got {other:?}"),
    });
    set_wasm_enabled(&extensions, WASM_FIXTURE, true).expect("opt the fixture in");

    // When a new version of it is installed over it
    let second = the_fixture_source_at_version("0.2.0");
    let replaced = install_from_path(&extensions, second.path()).map(|outcome| match outcome {
        InstallOutcome::Replaced { entry, .. } => entry.wasm_enabled,
        other => panic!("a new version is Replaced, got {other:?}"),
    });

    // Then a fresh install is not opted in (R1), and the new version keeps the user's opt-in
    assert_eq!((fresh, replaced), (Ok(false), Ok(true)));
}

//! Acceptance: `tddy-zed-extensions` installs Zed extensions into `<data dir>/extensions` in Zed's
//! installed layout, from a registry or a path, refuses unsafe or inconsistent archives without
//! touching what is installed, lists what each provides, and removes them (PRD acceptance criteria;
//! changeset R1–R6). No network: the registry is a local fixture server.

mod support;

use std::ffi::OsString;
use std::path::PathBuf;

use pretty_assertions::assert_eq;
use support::{a_data_dir, a_fixture_registry, the_demo_lang_archive, the_demo_lang_fixture};
use tddy_zed_extensions::{
    install_from_path, install_from_registry, list, remove, ExtensionError, ExtensionSource,
    ExtensionsDir, Index, InstallOutcome,
};

const DEMO: &str = "demo-lang";

#[tokio::test]
async fn installs_from_the_registry_into_the_installed_layout() {
    // Given
    let data = a_data_dir();
    let registry = a_fixture_registry()
        .await
        .serving(DEMO, "0.1.0", the_demo_lang_archive().build())
        .await;

    // When
    install_from_registry(&data.extensions, &registry.registry(), DEMO, Some("0.1.0"))
        .await
        .expect("installs");

    // Then
    assert_eq!(
        data.installed_files_of(DEMO),
        vec![
            "extension.toml",
            "grammars/demo.wasm",
            "languages/demo/config.toml",
        ]
    );
}

#[tokio::test]
async fn records_the_install_in_the_index() {
    // Given
    let data = a_data_dir();
    let registry = a_fixture_registry()
        .await
        .serving(DEMO, "0.1.0", the_demo_lang_archive().build())
        .await;

    // When
    install_from_registry(&data.extensions, &registry.registry(), DEMO, Some("0.1.0"))
        .await
        .expect("installs");

    // Then
    let entry = Index::load(&data.extensions)
        .expect("index loads")
        .extensions[DEMO]
        .clone();
    assert_eq!(
        (entry.id.as_str(), entry.version.as_str(), entry.source),
        (DEMO, "0.1.0", ExtensionSource::Registry)
    );
}

#[tokio::test]
async fn lists_what_an_extension_provides() {
    // Given
    let data = a_data_dir();
    let registry = a_fixture_registry()
        .await
        .serving(DEMO, "0.1.0", the_demo_lang_archive().build())
        .await;
    install_from_registry(&data.extensions, &registry.registry(), DEMO, Some("0.1.0"))
        .await
        .expect("installs");

    // When
    let lines: Vec<String> = list(&data.extensions)
        .expect("lists")
        .iter()
        .map(|installed| installed.line())
        .collect();

    // Then
    assert_eq!(
        lines,
        vec![
            "demo-lang 0.1.0 registry — languages: Demo; grammars: demo; language servers: none; wasm: no"
        ]
    );
}

#[test]
fn installs_a_dev_extension_from_a_path() {
    // Given
    let data = a_data_dir();

    // When
    let outcome = install_from_path(&data.extensions, &the_demo_lang_fixture()).expect("installs");

    // Then
    let InstallOutcome::Installed(entry) = outcome else {
        panic!("expected a fresh install, got {outcome:?}");
    };
    assert_eq!(
        (
            entry.id.as_str(),
            entry.version.as_str(),
            entry.source,
            data.installed_files_of(DEMO)
        ),
        (
            DEMO,
            "0.1.0",
            ExtensionSource::Path,
            vec![
                "extension.toml".to_string(),
                "grammars/demo.wasm".to_string(),
                "languages/demo/config.toml".to_string(),
            ]
        )
    );
}

#[tokio::test]
async fn refuses_an_archive_entry_that_climbs_out() {
    // Given
    let data = a_data_dir();
    let registry = a_fixture_registry()
        .await
        .serving_latest(
            DEMO,
            the_demo_lang_archive()
                .with_raw_entry("../escape.txt", b"outside")
                .build(),
        )
        .await;

    // When
    let refusal = install_from_registry(&data.extensions, &registry.registry(), DEMO, None).await;

    // Then
    assert_eq!(
        (
            refusal,
            data.extensions.root().join("escape.txt").exists(),
            data.installed_dir_names()
        ),
        (
            Err(ExtensionError::UnsafeArchiveEntry {
                extension: DEMO.into(),
                entry: PathBuf::from("../escape.txt"),
            }),
            false,
            Vec::<String>::new()
        )
    );
}

#[tokio::test]
async fn refuses_a_chain_of_symlinks_that_resolves_outside() {
    // Given: `a/b/up` -> `../..` (the extension root) and `out` -> `a/b/up/..`, each contained on
    // its face, together resolving to the directory above the extension.
    let data = a_data_dir();
    let registry = a_fixture_registry()
        .await
        .serving_latest(
            DEMO,
            the_demo_lang_archive()
                .with_symlink("a/b/up", "../..")
                .with_symlink("out", "a/b/up/..")
                .build(),
        )
        .await;

    // When
    let refusal = install_from_registry(&data.extensions, &registry.registry(), DEMO, None).await;

    // Then
    assert_eq!(
        (refusal, data.installed_dir_names()),
        (
            Err(ExtensionError::UnsafeArchiveEntry {
                extension: DEMO.into(),
                entry: PathBuf::from("out"),
            }),
            Vec::<String>::new()
        )
    );
}

#[tokio::test]
async fn refuses_a_language_config_that_does_not_parse() {
    // Given
    let data = a_data_dir();
    let registry = a_fixture_registry()
        .await
        .serving_latest(
            DEMO,
            the_demo_lang_archive()
                .with_file("languages/demo/config.toml", b"name = [unclosed")
                .build(),
        )
        .await;

    // When
    let refusal = install_from_registry(&data.extensions, &registry.registry(), DEMO, None).await;

    // Then
    assert_eq!(
        (
            matches!(
                &refusal,
                Err(ExtensionError::ManifestUnparsable { extension, file, .. })
                    if extension == DEMO && file == &PathBuf::from("languages/demo/config.toml")
            ),
            data.installed_dir_names()
        ),
        (true, Vec::<String>::new())
    );
}

#[tokio::test]
async fn refuses_a_grammar_the_archive_does_not_ship() {
    // Given
    let data = a_data_dir();
    let registry = a_fixture_registry()
        .await
        .serving(
            DEMO,
            "0.1.0",
            the_demo_lang_archive()
                .without("grammars/demo.wasm")
                .build(),
        )
        .await;

    // When
    let refusal =
        install_from_registry(&data.extensions, &registry.registry(), DEMO, Some("0.1.0")).await;

    // Then
    assert_eq!(
        (refusal, data.installed_dir_names()),
        (
            Err(ExtensionError::GrammarWasmMissing {
                extension: DEMO.into(),
                grammar: "demo".into(),
                file: PathBuf::from("grammars/demo.wasm"),
            }),
            Vec::<String>::new()
        )
    );
}

#[tokio::test]
async fn refuses_a_manifest_whose_id_differs() {
    // Given
    let data = a_data_dir();
    let registry = a_fixture_registry()
        .await
        .serving(
            DEMO,
            "0.1.0",
            the_demo_lang_archive().with_id("other-lang").build(),
        )
        .await;

    // When
    let refusal =
        install_from_registry(&data.extensions, &registry.registry(), DEMO, Some("0.1.0")).await;

    // Then
    assert_eq!(
        (refusal, data.installed_dir_names()),
        (
            Err(ExtensionError::IdMismatch {
                requested: DEMO.into(),
                found: "other-lang".into(),
                file: PathBuf::from("extension.toml"),
            }),
            Vec::<String>::new()
        )
    );
}

#[tokio::test]
async fn a_refused_install_keeps_the_previous_version() {
    // Given
    let data = a_data_dir();
    let registry = a_fixture_registry()
        .await
        .serving(DEMO, "0.1.0", the_demo_lang_archive().build())
        .await
        .serving(
            DEMO,
            "0.2.0",
            the_demo_lang_archive()
                .with_version("0.2.0")
                .without("grammars/demo.wasm")
                .build(),
        )
        .await;
    install_from_registry(&data.extensions, &registry.registry(), DEMO, Some("0.1.0"))
        .await
        .expect("first install");

    // When
    let refusal =
        install_from_registry(&data.extensions, &registry.registry(), DEMO, Some("0.2.0")).await;

    // Then
    assert_eq!(
        (
            refusal.is_err(),
            data.installed_version_of(DEMO),
            data.installed_dir_names(),
            Index::load(&data.extensions)
                .expect("index loads")
                .extensions[DEMO]
                .version
                .clone()
        ),
        (
            true,
            Some("0.1.0".to_string()),
            vec![DEMO.to_string()],
            "0.1.0".to_string()
        )
    );
}

#[tokio::test]
async fn reinstalling_the_same_version_changes_nothing() {
    // Given
    let data = a_data_dir();
    let registry = a_fixture_registry()
        .await
        .serving(DEMO, "0.1.0", the_demo_lang_archive().build())
        .await;
    install_from_registry(&data.extensions, &registry.registry(), DEMO, Some("0.1.0"))
        .await
        .expect("first install");
    let index_before = Index::load(&data.extensions).expect("index loads");

    // When
    let outcome =
        install_from_registry(&data.extensions, &registry.registry(), DEMO, Some("0.1.0"))
            .await
            .expect("second install");

    // Then
    assert_eq!(
        (outcome, Index::load(&data.extensions).expect("index loads")),
        (
            InstallOutcome::AlreadyInstalled(index_before.extensions[DEMO].clone()),
            index_before
        )
    );
}

#[tokio::test]
async fn reinstalling_a_different_version_replaces_it() {
    // Given
    let data = a_data_dir();
    let registry = a_fixture_registry()
        .await
        .serving(DEMO, "0.1.0", the_demo_lang_archive().build())
        .await
        .serving(
            DEMO,
            "0.2.0",
            the_demo_lang_archive().with_version("0.2.0").build(),
        )
        .await;
    install_from_registry(&data.extensions, &registry.registry(), DEMO, Some("0.1.0"))
        .await
        .expect("first install");

    // When
    let outcome =
        install_from_registry(&data.extensions, &registry.registry(), DEMO, Some("0.2.0"))
            .await
            .expect("second install");

    // Then
    let InstallOutcome::Replaced {
        previous_version,
        entry,
    } = outcome
    else {
        panic!("expected a replacement, got {outcome:?}");
    };
    assert_eq!(
        (
            previous_version.as_str(),
            entry.version.as_str(),
            data.installed_version_of(DEMO)
        ),
        ("0.1.0", "0.2.0", Some("0.2.0".to_string()))
    );
}

#[test]
fn removes_an_extension_and_its_index_entry() {
    // Given
    let data = a_data_dir();
    install_from_path(&data.extensions, &the_demo_lang_fixture()).expect("installs");

    // When
    let removed = remove(&data.extensions, DEMO).expect("removes");

    // Then
    assert_eq!(
        (
            removed.id.as_str(),
            data.extensions.extension(DEMO).exists(),
            Index::load(&data.extensions)
                .expect("index loads")
                .extensions
                .contains_key(DEMO)
        ),
        (DEMO, false, false)
    );
}

#[test]
fn removing_an_unknown_id_names_it() {
    // Given
    let data = a_data_dir();

    // When
    let refusal = remove(&data.extensions, "no-such-ext");

    // Then
    assert_eq!(
        refusal,
        Err(ExtensionError::NotInstalled {
            extension: "no-such-ext".into()
        })
    );
}

#[test]
fn everything_happens_under_tddy_data_dir() {
    // Given
    let data = a_data_dir();
    let home = a_data_dir();
    let dir = ExtensionsDir::resolve_from(
        Some(OsString::from(data.path())),
        Some(OsString::from(home.path())),
    )
    .expect("resolves");

    // When
    install_from_path(&dir, &the_demo_lang_fixture()).expect("installs");

    // Then
    assert_eq!(
        (
            dir.root().to_path_buf(),
            data.path()
                .join("extensions/installed/demo-lang/extension.toml")
                .is_file(),
            data.path().join("extensions/index.json").is_file(),
            home.path().read_dir().expect("read home").count()
        ),
        (data.path().join("extensions"), true, true, 0)
    );
}

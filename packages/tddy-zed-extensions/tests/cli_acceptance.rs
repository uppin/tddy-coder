//! Acceptance: `tddy-tools extensions install|list|remove` as a user types them, run against a temp
//! data dir and a local fixture registry, print what each command did.

mod support;

use clap::Parser;
use pretty_assertions::assert_eq;
use support::{a_data_dir, a_fixture_registry, the_demo_lang_archive, the_demo_lang_fixture};
use tddy_zed_extensions::cli::{execute, ExtensionsArgs};
use tddy_zed_extensions::Registry;

const DEMO: &str = "demo-lang";

fn command(argv: &[&str]) -> tddy_zed_extensions::cli::ExtensionsCommand {
    ExtensionsArgs::try_parse_from(argv)
        .expect("the command parses")
        .command
}

#[tokio::test]
async fn install_list_and_remove_print_what_they_did() {
    // Given
    let data = a_data_dir();
    let fixture_registry = a_fixture_registry()
        .await
        .serving(DEMO, "0.1.0", the_demo_lang_archive().build())
        .await;
    let registry = fixture_registry.registry();

    // When
    let mut printed = Vec::new();
    for argv in [
        &["extensions", "list"][..],
        &["extensions", "install", "demo-lang@0.1.0"],
        &["extensions", "install", "demo-lang@0.1.0"],
        &["extensions", "remove", "demo-lang"],
    ] {
        printed.push(
            execute(&data.extensions, &registry, command(argv))
                .await
                .expect("the command succeeds"),
        );
    }

    // Then
    assert_eq!(
        printed,
        vec![
            vec!["no extensions installed".to_string()],
            vec!["installed demo-lang 0.1.0".to_string()],
            vec!["demo-lang 0.1.0 is already installed; nothing changed".to_string()],
            vec!["removed demo-lang".to_string()],
        ]
    );
}

#[tokio::test]
async fn install_from_a_path_then_list_prints_one_line_per_extension() {
    // Given
    let data = a_data_dir();
    let fixture = the_demo_lang_fixture();
    let unused = Registry::at("http://127.0.0.1:9");

    // When
    let installed = execute(
        &data.extensions,
        &unused,
        command(&["extensions", "install", "--path", fixture.to_str().unwrap()]),
    )
    .await
    .expect("the install succeeds");
    let listed = execute(&data.extensions, &unused, command(&["extensions", "list"]))
        .await
        .expect("the list succeeds");

    // Then
    assert_eq!(
        (
            installed.len(),
            listed.len(),
            listed[0].starts_with("demo-lang 0.1.0 path — ")
        ),
        (1, 1, true)
    );
}

#[tokio::test]
async fn an_empty_version_after_the_at_is_refused() {
    // Given
    let data = a_data_dir();

    // When
    let refusal = execute(
        &data.extensions,
        &Registry::at("http://127.0.0.1:9"),
        command(&["extensions", "install", "demo-lang@"]),
    )
    .await
    .map_err(|e| e.to_string());

    // Then
    assert_eq!(
        refusal,
        Err("demo-lang@: the version after @ is empty".to_string())
    );
}

//! Unit tests for the repo-root `.env` reader.
//!
//! These are pure — they parse text and never touch the process environment — so they run
//! in the default `./test` suite.

use pretty_assertions::assert_eq;
use tddy_vm_testkit::env_file::parse_env_file;

/// The parsed pairs as `KEY=VALUE` strings, so an assertion reads as the file does.
fn pairs_of(contents: &str) -> Vec<String> {
    parse_env_file(contents)
        .into_iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect()
}

#[test]
fn reads_the_base_image_path_a_developer_pointed_at_an_on_disk_image() {
    // Given a .env naming an image already on the developer's disk
    let contents = "TDDY_CLOUDINIT_BASE_IMAGE=/Users/dev/vm/debian-12-genericcloud-arm64.qcow2\n";

    // When it is parsed
    let pairs = pairs_of(contents);

    // Then the path arrives intact
    assert_eq!(
        pairs,
        vec!["TDDY_CLOUDINIT_BASE_IMAGE=/Users/dev/vm/debian-12-genericcloud-arm64.qcow2"]
    );
}

#[test]
fn ignores_comments_and_blank_lines() {
    // Given a .env written the way people actually write them
    let contents = "\
# the base image every bake starts from
TDDY_CLOUDINIT_BASE_IMAGE=/vm/base.qcow2

# nothing below here yet
";

    // When it is parsed
    let pairs = pairs_of(contents);

    // Then only the assignment survives
    assert_eq!(pairs, vec!["TDDY_CLOUDINIT_BASE_IMAGE=/vm/base.qcow2"]);
}

#[test]
fn strips_the_quotes_a_shell_would_have_consumed() {
    // Given values quoted both ways, as `.env.example` files habitually are
    let contents = "DOUBLE=\"/vm/base.qcow2\"\nSINGLE='/vm/other.qcow2'\n";

    // When it is parsed
    let pairs = pairs_of(contents);

    // Then the quotes are gone, matching what `./web-dev` exports for the same file
    assert_eq!(
        pairs,
        vec!["DOUBLE=/vm/base.qcow2", "SINGLE=/vm/other.qcow2"]
    );
}

#[test]
fn keeps_every_equals_sign_after_the_first_one_in_the_value() {
    // Given a value that itself contains `=`
    let contents = "TDDY_EXTRA_ARGS=--flag=value\n";

    // When it is parsed
    let pairs = pairs_of(contents);

    // Then only the first `=` separates key from value, as `IFS='=' read -r key value` does
    assert_eq!(pairs, vec!["TDDY_EXTRA_ARGS=--flag=value"]);
}

#[test]
fn reports_a_key_with_no_value_as_an_empty_string_rather_than_dropping_it() {
    // Given a key deliberately blanked out
    let contents = "TDDY_CLOUDINIT_BASE_IMAGE=\n";

    // When it is parsed
    let pairs = pairs_of(contents);

    // Then it is still present — the caller decides that an empty value means "unset",
    // exactly as the testkit treats an empty env var
    assert_eq!(pairs, vec!["TDDY_CLOUDINIT_BASE_IMAGE="]);
}

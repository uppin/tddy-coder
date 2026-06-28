//! Unit: the runner resolves out-of-band `TDDY_SECRET_*` file references into real env vars for the
//! inner Claude child, and unlinks the secret files so they don't linger in scratch.

use std::collections::BTreeMap;

use tddy_sandbox_runner::resolve_secret_envs;

#[test]
fn resolves_a_secret_file_into_the_real_env_var_and_unlinks_it() {
    // Given — a TDDY_SECRET_<NAME> entry pointing at a file holding the secret value
    let tmp = tempfile::tempdir().unwrap();
    let secret_file = tmp.path().join("oauth_token");
    std::fs::write(&secret_file, "tok-abc-123").unwrap();
    let mut vars = BTreeMap::new();
    vars.insert(
        "TDDY_SECRET_CLAUDE_CODE_OAUTH_TOKEN".to_string(),
        secret_file.to_string_lossy().to_string(),
    );
    vars.insert("HOME".to_string(), "/home/x".to_string());

    // When
    let resolved = resolve_secret_envs(&vars);

    // Then — the real env var carries the file contents…
    assert_eq!(
        resolved,
        vec![(
            "CLAUDE_CODE_OAUTH_TOKEN".to_string(),
            "tok-abc-123".to_string()
        )]
    );
    // …and the secret file is unlinked.
    assert!(
        !secret_file.exists(),
        "secret file must be unlinked after resolution"
    );
}

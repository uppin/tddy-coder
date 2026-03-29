//! Red: client-side sandbox path normalization mirrors daemon VFS rules.

#[test]
fn normalize_accepts_simple_relative_path() {
    let p = tddy_remote::normalize_sandbox_relative_path("work/src/file.txt").expect("normalize");
    assert_eq!(p, std::path::Path::new("work/src/file.txt"));
}

#[test]
fn normalize_rejects_parent_segments() {
    let err = tddy_remote::normalize_sandbox_relative_path("a/../../etc/passwd").unwrap_err();
    assert!(
        err.contains("traversal") || err.contains(".."),
        "expected traversal rejection message, got {err:?}"
    );
}

//! `#fs-port`: this crate reaches disk only through the filesystem port (`tddy-fs`).

use tddy_fs::Fs;

#[test]
fn the_channel_manifest_is_built_through_the_fs_port() {
    // Given — a session directory that exists only in memory.
    let fs = tddy_fs::MemFs::new();
    let session_dir = std::path::Path::new("/sessions/s-1");

    // When — the manifest is built with session defaults through the port.
    let manifest = tddy_session_actions::session_action_pipeline::build_extended_channel_manifest_on(
        &fs,
        session_dir,
        None,
        None,
    )
    .expect("a channel manifest");

    // Then — stdout/stderr default under `capture/`, `logs` names the logs dir, and both
    // directories exist in memory rather than on disk.
    assert_eq!(
        manifest.get("stdout"),
        Some(&session_dir.join("capture").join("stdout.txt")),
        "stdout defaults under capture/"
    );
    assert_eq!(
        manifest.get("stderr"),
        Some(&session_dir.join("capture").join("stderr.txt")),
        "stderr defaults under capture/"
    );
    assert_eq!(
        manifest.get("logs"),
        Some(&session_dir.join("logs")),
        "logs names the session logs dir"
    );
    assert_eq!(
        fs.metadata(&session_dir.join("capture")).expect("capture").kind,
        tddy_fs::FsKind::Dir,
        "the capture dir is created through the port"
    );
    assert_eq!(
        fs.metadata(&session_dir.join("logs")).expect("logs").kind,
        tddy_fs::FsKind::Dir,
        "the logs dir is created through the port"
    );

    // When — the caller overrides both capture paths.
    let custom_out = std::path::Path::new("/elsewhere/out.txt");
    let custom_err = std::path::Path::new("/elsewhere/nested/err.txt");
    let overridden =
        tddy_session_actions::session_action_pipeline::build_extended_channel_manifest_on(
            &fs,
            session_dir,
            Some(custom_out),
            Some(custom_err),
        )
        .expect("an overridden channel manifest");

    // Then — the overrides win, byte for byte.
    assert_eq!(overridden.get("stdout"), Some(&custom_out.to_path_buf()));
    assert_eq!(overridden.get("stderr"), Some(&custom_err.to_path_buf()));
}

#[test]
fn production_code_reaches_disk_only_through_the_fs_port() {
    let hits =
        tddy_fs::testing::direct_disk_io_in(std::path::Path::new(env!("CARGO_MANIFEST_DIR")));
    assert!(
        hits.is_empty(),
        "direct disk I/O left in production code:\n{}",
        hits.join("\n")
    );
}

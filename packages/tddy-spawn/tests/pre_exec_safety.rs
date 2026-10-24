//! Acceptance: no daemon spawn path touches the process environment or NSS between `fork` and
//! `exec`. `std::env::set_var` there waits forever on the environment lock `Command::spawn` holds
//! across the fork (the 2026-10-24 incident); `initgroups(3)` enters NSS after the fork.
//!
//! The spawn paths are read as source: both calls are a defect wherever they appear in these
//! files, since every privilege drop now goes through `tddy_daemon_kernel::pre_exec_stages`.
//!
//! Feature docs: docs/ft/daemon/1-WIP/PRD-2026-10-24-start-progress-spawn-diag.md

const SPAWN_PATHS: [(&str, &str); 2] = [
    (
        "tddy-spawn/src/spawner.rs",
        include_str!("../src/spawner.rs"),
    ),
    (
        "tddy-daemon-kernel/src/spawn_as_user.rs",
        include_str!("../../tddy-daemon-kernel/src/spawn_as_user.rs"),
    ),
];

fn code_lines(source: &str) -> impl Iterator<Item = (usize, &str)> {
    source
        .lines()
        .enumerate()
        .map(|(index, line)| (index + 1, line))
        .filter(|(_, line)| !line.trim_start().starts_with("//"))
}

#[test]
fn no_spawn_path_sets_the_environment_or_calls_initgroups() {
    // When
    let offenders: Vec<String> = SPAWN_PATHS
        .iter()
        .flat_map(|(file, source)| {
            code_lines(source)
                .filter(|(_, line)| line.contains("set_var(") || line.contains("initgroups("))
                .map(move |(number, line)| format!("{file}:{number}: {}", line.trim()))
        })
        .collect();

    // Then
    assert!(
        offenders.is_empty(),
        "spawn paths must drop privileges through pre_exec_stages, not by hand:\n{}",
        offenders.join("\n")
    );
}

#[test]
fn every_spawn_path_goes_through_staged_command() {
    for (file, source) in SPAWN_PATHS {
        assert!(
            source.contains("StagedCommand"),
            "{file} starts its children through pre_exec_stages::StagedCommand"
        );
    }
}

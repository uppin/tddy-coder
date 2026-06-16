//! Lower a declared [`BuildTarget`] into the concrete [`BuildAction`]s that build it.
//!
//! Typed configs translate to canonical argv:
//! - `rust_binary` / `rust_library` → `cargo build -p <pkg> [...]`
//! - `typescript` → `bun run <script>` (cwd = `package_dir`)
//! - `docker_image` → `docker build [...]`
//! - `script` → the declared command
//! - `tool` → no build action (registers its `bin_dir` for dependents)
//! - `group` → no own action (members build via the graph)
//!
//! Any explicit `actions` declared on the target are included first, as-is.

use crate::error::BuildError;
use crate::proto::{
    build_target::Config, ActionType, BuildAction, BuildTarget, DockerImageTarget,
    RustBinaryTarget, RustLibraryTarget, ScriptTarget, TypeScriptTarget,
};

/// Produce the ordered actions for a single target.
pub fn lower_target(target: &BuildTarget) -> Result<Vec<BuildAction>, BuildError> {
    let mut actions = target.actions.clone();

    if let Some(config) = &target.config {
        match config {
            Config::RustBinary(rb) => actions.push(rust_binary_action(rb)),
            Config::RustLibrary(rl) => actions.push(rust_library_action(rl)),
            Config::Typescript(ts) => actions.push(typescript_action(ts)),
            Config::DockerImage(d) => actions.push(docker_action(d)),
            Config::Script(s) => actions.push(script_action(s)),
            Config::Tool(_) | Config::Group(_) => {}
        }
    }

    Ok(actions)
}

fn command_action(id: &str, description: String, command: Vec<String>) -> BuildAction {
    BuildAction {
        id: id.to_string(),
        description,
        r#type: ActionType::Command as i32,
        command,
        ..Default::default()
    }
}

fn rust_binary_action(rb: &RustBinaryTarget) -> BuildAction {
    let mut command = vec![
        "cargo".to_string(),
        "build".to_string(),
        "-p".to_string(),
        rb.package.clone(),
    ];
    if !rb.bin_name.is_empty() {
        command.push("--bin".to_string());
        command.push(rb.bin_name.clone());
    }
    push_features(&mut command, &rb.features);
    push_profile(&mut command, &rb.profile);
    if !rb.target_triple.is_empty() {
        command.push("--target".to_string());
        command.push(rb.target_triple.clone());
    }
    command_action(
        "rust-binary",
        format!("cargo build {}", rb.package),
        command,
    )
}

fn rust_library_action(rl: &RustLibraryTarget) -> BuildAction {
    let mut command = vec![
        "cargo".to_string(),
        "build".to_string(),
        "-p".to_string(),
        rl.package.clone(),
    ];
    push_features(&mut command, &rl.features);
    push_profile(&mut command, &rl.profile);
    command_action(
        "rust-library",
        format!("cargo build {}", rl.package),
        command,
    )
}

fn typescript_action(ts: &TypeScriptTarget) -> BuildAction {
    let script = if ts.build_script.is_empty() {
        "build".to_string()
    } else {
        ts.build_script.clone()
    };
    let mut action = command_action(
        "typescript",
        format!("bun run {script}"),
        vec!["bun".to_string(), "run".to_string(), script],
    );
    action.working_dir = ts.package_dir.clone();
    action
}

fn docker_action(d: &DockerImageTarget) -> BuildAction {
    let mut command = vec!["docker".to_string(), "build".to_string()];
    if !d.dockerfile.is_empty() {
        command.push("-f".to_string());
        command.push(d.dockerfile.clone());
    }
    if !d.tag.is_empty() {
        command.push("-t".to_string());
        command.push(d.tag.clone());
    }
    for arg in &d.build_args {
        command.push("--build-arg".to_string());
        command.push(arg.clone());
    }
    command.push(if d.context.is_empty() {
        ".".to_string()
    } else {
        d.context.clone()
    });
    command_action("docker-image", format!("docker build {}", d.tag), command)
}

fn script_action(s: &ScriptTarget) -> BuildAction {
    let mut action = command_action("script", "script".to_string(), s.command.clone());
    action.env = s.env.clone();
    if !s.working_dir_rel.is_empty() {
        action.working_dir = s.working_dir_rel.join("/");
    }
    action
}

fn push_features(command: &mut Vec<String>, features: &[String]) {
    if !features.is_empty() {
        command.push("--features".to_string());
        command.push(features.join(","));
    }
}

fn push_profile(command: &mut Vec<String>, profile: &str) {
    if profile == "release" {
        command.push("--release".to_string());
    }
}

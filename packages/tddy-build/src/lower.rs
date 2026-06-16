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

#[cfg(test)]
mod tests {
    use super::lower_target;
    use crate::proto::{
        build_target::Config, ActionType, BuildAction, BuildTarget, DockerImageTarget,
        RustBinaryTarget, RustLibraryTarget, ScriptTarget, TargetGroupTarget, ToolTarget,
        TypeScriptTarget,
    };

    fn target(config: Config) -> BuildTarget {
        BuildTarget {
            id: "t".to_string(),
            config: Some(config),
            ..Default::default()
        }
    }

    fn only(config: Config) -> BuildAction {
        let mut actions = lower_target(&target(config)).unwrap();
        assert_eq!(actions.len(), 1, "expected exactly one lowered action");
        actions.pop().unwrap()
    }

    #[test]
    fn rust_binary_builds_cargo_argv_with_features_profile_and_triple() {
        let action = only(Config::RustBinary(RustBinaryTarget {
            package: "app".to_string(),
            bin_name: "app".to_string(),
            features: vec!["a".to_string(), "b".to_string()],
            profile: "release".to_string(),
            target_triple: "x86_64-unknown-linux-gnu".to_string(),
        }));
        assert_eq!(
            action.command,
            vec![
                "cargo",
                "build",
                "-p",
                "app",
                "--bin",
                "app",
                "--features",
                "a,b",
                "--release",
                "--target",
                "x86_64-unknown-linux-gnu",
            ]
        );
        assert_eq!(action.r#type, ActionType::Command as i32);
    }

    #[test]
    fn rust_library_omits_release_for_debug_profile() {
        let action = only(Config::RustLibrary(RustLibraryTarget {
            package: "core".to_string(),
            profile: "debug".to_string(),
            ..Default::default()
        }));
        assert_eq!(action.command, vec!["cargo", "build", "-p", "core"]);
    }

    #[test]
    fn typescript_runs_bun_in_package_dir() {
        let action = only(Config::Typescript(TypeScriptTarget {
            package_dir: "packages/web".to_string(),
            build_script: "build".to_string(),
            ..Default::default()
        }));
        assert_eq!(action.command, vec!["bun", "run", "build"]);
        assert_eq!(action.working_dir, "packages/web");
    }

    #[test]
    fn docker_builds_with_dockerfile_tag_args_and_context() {
        let action = only(Config::DockerImage(DockerImageTarget {
            dockerfile: "Dockerfile".to_string(),
            context: "ctx".to_string(),
            tag: "img:latest".to_string(),
            build_args: vec!["A=1".to_string()],
        }));
        assert_eq!(
            action.command,
            vec![
                "docker",
                "build",
                "-f",
                "Dockerfile",
                "-t",
                "img:latest",
                "--build-arg",
                "A=1",
                "ctx",
            ]
        );
    }

    #[test]
    fn script_passes_command_env_and_working_dir() {
        let action = only(Config::Script(ScriptTarget {
            command: vec!["echo".to_string(), "hi".to_string()],
            env: std::collections::HashMap::from([("K".to_string(), "V".to_string())]),
            working_dir_rel: vec!["sub".to_string(), "dir".to_string()],
        }));
        assert_eq!(action.command, vec!["echo", "hi"]);
        assert_eq!(action.working_dir, "sub/dir");
        assert_eq!(action.env.get("K").map(String::as_str), Some("V"));
    }

    #[test]
    fn tool_and_group_produce_no_own_actions() {
        assert!(lower_target(&target(Config::Tool(ToolTarget {
            bin_dir: "bin".to_string(),
            ..Default::default()
        })))
        .unwrap()
        .is_empty());
        assert!(lower_target(&target(Config::Group(TargetGroupTarget {
            member_ids: vec!["a".to_string()],
        })))
        .unwrap()
        .is_empty());
    }

    #[test]
    fn explicit_actions_precede_lowered_config_action() {
        let explicit = BuildAction {
            id: "pre".to_string(),
            r#type: ActionType::Command as i32,
            command: vec!["true".to_string()],
            ..Default::default()
        };
        let mut t = target(Config::Script(ScriptTarget {
            command: vec!["echo".to_string()],
            ..Default::default()
        }));
        t.actions = vec![explicit];
        let actions = lower_target(&t).unwrap();
        assert_eq!(actions.len(), 2);
        assert_eq!(actions[0].id, "pre");
        assert_eq!(actions[1].command, vec!["echo"]);
    }
}

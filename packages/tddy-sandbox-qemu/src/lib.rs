pub mod argv;
pub mod cli;
pub mod spawn;

pub use cli::{parse_env_vars, parse_mount_spec, SandboxQemuArgs};
pub use spawn::{spawn_plan, spawn_plan_with, QemuBackendOptions};

/// Build a `SandboxPlan` from CLI args and boot it via [`spawn::spawn_plan_with`].
///
/// Will return the guest command's actual exit code once connecting to the in-guest
/// runner is implemented (see `docs/dev/1-WIP/qemu-sandbox-cli.md` "Open design points").
/// Until then, a successful boot is deliberately reported as an error rather than a fake
/// `0` — the guest command was never actually run, so exit code `0` would be a lie.
pub async fn run_sandbox_qemu(args: cli::SandboxQemuArgs) -> anyhow::Result<i32> {
    let mounts = args
        .mounts
        .iter()
        .map(|m| cli::parse_mount_spec(m))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| anyhow::anyhow!(e))?;
    let env_vars = cli::parse_env_vars(&args.env).map_err(|e| anyhow::anyhow!(e))?;

    let scratch = tempfile::tempdir()?;
    let egress_dir = scratch.path().join("egress");
    tokio::fs::create_dir_all(&egress_dir).await?;

    let plan = tddy_sandbox::SandboxBuilder::new(
        std::env::current_dir()?,
        scratch.path().to_path_buf(),
        egress_dir,
        args.command.clone(),
    )
    .mounts(mounts)
    .env_map(env_vars)
    .cwd(args.cwd.clone())
    .build()
    .map_err(|e| anyhow::anyhow!("{e}"))?;

    let overlay_path = scratch.path().join("overlay.qcow2");
    let runner_dir_path = args.runner_dir.clone().unwrap_or_default();

    match spawn::spawn_plan_with(
        plan,
        spawn::QemuBackendOptions {
            image_path: args.image.clone(),
            overlay_path,
            runner_dir_path,
            control_port: spawn::DEFAULT_CONTROL_PORT,
        },
    ) {
        Ok(mut handle) => {
            let _ = handle.child_mut().kill();
            eprintln!(
                "Error: VM booted but connecting to the in-guest runner and executing the \
                 command is not yet implemented"
            );
            Ok(1)
        }
        Err(err) => {
            eprintln!("Error: {err}");
            Ok(1)
        }
    }
}

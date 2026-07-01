pub mod argv;
pub mod cli;
pub mod spawn;

pub use cli::{parse_env_vars, parse_mount_spec, SandboxQemuArgs};
pub use spawn::{spawn_plan, spawn_plan_with, QemuBackendOptions};

/// Wait for `handle.ready_marker_path` to appear (written by the background task in
/// [`spawn::spawn_plan_with`] once the guest's control port answers), or for the guest VM
/// process to exit early, whichever happens first.
async fn wait_for_ready_marker(
    handle: &mut tddy_sandbox::SandboxHandle,
    timeout: std::time::Duration,
) -> anyhow::Result<()> {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        if handle.ready_marker_path.exists() {
            return Ok(());
        }
        if let Some(diagnostic) = handle.try_exit_diagnostic() {
            anyhow::bail!("guest VM exited before becoming ready: {diagnostic}");
        }
        if tokio::time::Instant::now() >= deadline {
            anyhow::bail!(
                "timed out after {timeout:?} waiting for the in-guest runner to become ready"
            );
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
}

/// Build a `SandboxPlan` from CLI args, boot it via [`spawn::spawn_plan_with`], connect to
/// the in-guest `tddy-sandbox-runner` once its control port answers, and stream the guest
/// command's terminal output to stdout via the shared [`tddy_sandbox_runner::run_host_relay`]
/// driver (the same one darwin/cgroups use).
///
/// The `SessionChannel` protocol carries an exit code in `SessionEnded`, but
/// `run_host_relay` doesn't currently surface it to callers (see
/// `docs/dev/1-WIP/qemu-sandbox-cli.md` "Open design points" — plumbing it through touches
/// shared, production sandbox infrastructure used by darwin/cgroups too, so it's tracked
/// separately rather than done as a quick, riskier edit here). Until then, this reports
/// `0` once the session ends cleanly and `1` on any connection/boot/relay failure — an
/// approximation, not the guest command's real exit code.
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

    let mut handle = match spawn::spawn_plan_with(
        plan,
        spawn::QemuBackendOptions {
            image_path: args.image.clone(),
            overlay_path,
            runner_dir_path,
            control_port: spawn::DEFAULT_CONTROL_PORT,
        },
    ) {
        Ok(handle) => handle,
        Err(err) => {
            eprintln!("Error: {err}");
            return Ok(1);
        }
    };

    if let Err(e) = wait_for_ready_marker(&mut handle, std::time::Duration::from_secs(180)).await {
        let _ = handle.child_mut().kill();
        eprintln!("Error: {e}");
        return Ok(1);
    }

    let client = match tddy_sandbox_runner::connect_sandbox_client(&handle.ready_marker_path).await
    {
        Ok(client) => client,
        Err(e) => {
            let _ = handle.child_mut().kill();
            eprintln!("Error: failed to connect to in-guest runner: {e}");
            return Ok(1);
        }
    };

    let (term_tx, mut term_rx) = tokio::sync::mpsc::unbounded_channel::<bytes::Bytes>();
    let relay = match tddy_sandbox_runner::run_host_relay(
        client,
        tddy_sandbox_runner::NullToolHandler,
        tddy_sandbox_runner::HostRelayConfig::new("tddy-sandbox-qemu", term_tx),
        tokio::sync::mpsc::unbounded_channel().1,
    )
    .await
    {
        Ok(relay) => relay,
        Err(e) => {
            let _ = handle.child_mut().kill();
            eprintln!("Error: failed to start session with in-guest runner: {e}");
            return Ok(1);
        }
    };

    use tokio::io::AsyncWriteExt;
    let mut stdout = tokio::io::stdout();
    while let Some(chunk) = term_rx.recv().await {
        let _ = stdout.write_all(&chunk).await;
    }
    let _ = stdout.flush().await;

    let relay_result = relay.await;
    let _ = handle.child_mut().kill();

    match relay_result {
        Ok(()) => Ok(0),
        Err(e) => {
            eprintln!("Error: host relay task failed: {e}");
            Ok(1)
        }
    }
}

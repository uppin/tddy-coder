use clap::Parser;
use std::collections::BTreeMap;
use std::path::PathBuf;
use tddy_sandbox::MountSpec;

#[derive(Parser, Debug, Clone)]
#[command(name = "tddy-sandbox-qemu")]
#[command(
    about = "Run a QEMU VM image as a tddy sandbox: mount host directories and execute a command in the guest"
)]
pub struct SandboxQemuArgs {
    /// Path to the qcow2 image to boot.
    #[arg(long)]
    pub image: PathBuf,

    /// Host directory to mount into the guest: HOST:JAIL[:rw]. Repeatable.
    #[arg(long = "mount", value_name = "HOST:JAIL[:rw]")]
    pub mounts: Vec<String>,

    /// Environment variable to set in the guest: KEY=VALUE. Repeatable.
    #[arg(long = "env", value_name = "KEY=VALUE")]
    pub env: Vec<String>,

    /// Working directory inside the guest.
    #[arg(long)]
    pub cwd: Option<PathBuf>,

    /// Directory containing the cross-compiled `tddy-sandbox-runner` binary for the
    /// guest (9p-mounted under the reserved runner tag).
    #[arg(long, env = "TDDY_SANDBOX_QEMU_RUNNER_DIR")]
    pub runner_dir: Option<PathBuf>,

    /// Command to run inside the guest.
    #[arg(trailing_var_arg = true, required = true)]
    pub command: Vec<String>,
}

/// Parse a `--mount HOST:JAIL[:rw]` value into a `MountSpec`. Read-only unless the value
/// carries a trailing `:rw` segment.
pub fn parse_mount_spec(raw: &str) -> Result<MountSpec, String> {
    let parts: Vec<&str> = raw.split(':').collect();
    match parts.as_slice() {
        [host, jail] => Ok(MountSpec {
            host: PathBuf::from(host),
            jail: Some(PathBuf::from(jail)),
            writable: false,
        }),
        [host, jail, "rw"] => Ok(MountSpec {
            host: PathBuf::from(host),
            jail: Some(PathBuf::from(jail)),
            writable: true,
        }),
        _ => Err(format!(
            "invalid --mount value '{raw}': expected HOST:JAIL[:rw]"
        )),
    }
}

/// Parse repeated `--env KEY=VALUE` values into a map.
pub fn parse_env_vars(raw: &[String]) -> Result<BTreeMap<String, String>, String> {
    let mut map = BTreeMap::new();
    for entry in raw {
        let (key, value) = entry
            .split_once('=')
            .ok_or_else(|| format!("invalid --env value '{entry}': expected KEY=VALUE"))?;
        map.insert(key.to_string(), value.to_string());
    }
    Ok(map)
}

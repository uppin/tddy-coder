//! Host-level machine stats surfaced by the Host Stats Footer: per-core CPU utilization and the
//! free/total disk capacity of the daemon's default project directory.
//!
//! PRD: `docs/ft/web/host-stats-footer.md`.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// One mounted filesystem and its capacity, as reported by the host.
pub struct MountUsage {
    /// Absolute mount point (e.g. `/`, `/home`).
    pub mount_point: PathBuf,
    /// Free bytes on the filesystem.
    pub available_bytes: u64,
    /// Total bytes of the filesystem.
    pub total_bytes: u64,
}

/// Free/total disk capacity for the daemon's default project directory.
pub struct DiskUsage {
    /// Free bytes on the filesystem containing the project directory.
    pub available_bytes: u64,
    /// Total bytes of that filesystem.
    pub total_bytes: u64,
    /// Absolute path whose filesystem these figures describe.
    pub project_dir: String,
}

/// Select the mount whose `mount_point` is the longest *path-component* prefix of `target`.
///
/// Matching is component-aware, not string-prefix based: `/ho` does not match `/home/dev`, and
/// `/home` beats `/` for `/home/dev/repos`. Returns `None` when no mount is a prefix of `target`.
pub fn select_mount_for_path<'a>(
    mounts: &'a [MountUsage],
    target: &Path,
) -> Option<&'a MountUsage> {
    mounts
        .iter()
        .filter(|m| is_component_prefix(&m.mount_point, target))
        .max_by_key(|m| m.mount_point.components().count())
}

/// True when every component of `prefix` matches the leading components of `target`, in order.
fn is_component_prefix(prefix: &Path, target: &Path) -> bool {
    let mut target_components = target.components();
    for prefix_component in prefix.components() {
        match target_components.next() {
            Some(component) if component == prefix_component => {}
            _ => return false,
        }
    }
    true
}

/// Host machine stats provider. Injected into the connection service so tests can substitute a
/// deterministic fake for the live `sysinfo`-backed implementation.
pub trait HostStats: Send + Sync {
    /// Utilization percentage (0..100) of each logical core, core 0 first.
    fn cpu_per_core_percent(&self) -> Vec<f32>;
    /// Free/total capacity of the filesystem holding the daemon's default project directory.
    fn disk_for_project_dir(&self) -> DiskUsage;
}

/// Live host stats backed by the `sysinfo` crate.
///
/// Long-lived: constructed once in the connection service so successive CPU refreshes (~5 s apart,
/// driven by the web poll) observe real per-core deltas. The very first CPU sample reads ~0 for
/// every core because there is no prior sample to diff against — that is acceptable.
pub struct SysinfoHostStats {
    /// The daemon's default project directory; its filesystem is the one the footer reports on.
    project_dir: PathBuf,
    /// CPU sampling state. `sysinfo` computes per-core usage as the delta between two refreshes, so
    /// this must persist across calls.
    system: Mutex<sysinfo::System>,
}

impl SysinfoHostStats {
    /// Build a live provider reporting disk usage for `project_dir`'s filesystem.
    pub fn new(project_dir: PathBuf) -> Self {
        let mut system = sysinfo::System::new();
        // Prime the CPU sampler so the first real call already has a prior sample to diff against.
        system.refresh_cpu_usage();
        Self {
            project_dir,
            system: Mutex::new(system),
        }
    }
}

impl HostStats for SysinfoHostStats {
    fn cpu_per_core_percent(&self) -> Vec<f32> {
        let mut system = self.system.lock().expect("host stats CPU mutex poisoned");
        system.refresh_cpu_usage();
        system.cpus().iter().map(|cpu| cpu.cpu_usage()).collect()
    }

    fn disk_for_project_dir(&self) -> DiskUsage {
        let disks = sysinfo::Disks::new_with_refreshed_list();
        let mounts: Vec<MountUsage> = disks
            .iter()
            .map(|disk| MountUsage {
                mount_point: disk.mount_point().to_path_buf(),
                available_bytes: disk.available_space(),
                total_bytes: disk.total_space(),
            })
            .collect();

        let project_dir = self.project_dir.to_string_lossy().into_owned();

        if let Some(mount) = select_mount_for_path(&mounts, &self.project_dir) {
            return DiskUsage {
                available_bytes: mount.available_bytes,
                total_bytes: mount.total_bytes,
                project_dir,
            };
        }

        // No mount is a component prefix of the project directory (unusual — the root `/` mount
        // normally matches every absolute path). Report the largest filesystem by total capacity
        // so the footer still shows a meaningful figure rather than misleading zeros. Zeros are
        // only reported when the host advertises no mounts at all.
        // TODO(host-stats-footer): revisit whether a non-matching project dir should surface an
        // explicit error to the web instead of falling back to the largest mount.
        match mounts.iter().max_by_key(|m| m.total_bytes) {
            Some(mount) => DiskUsage {
                available_bytes: mount.available_bytes,
                total_bytes: mount.total_bytes,
                project_dir,
            },
            None => DiskUsage {
                available_bytes: 0,
                total_bytes: 0,
                project_dir,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A mount at `mount_point` with distinguishable capacity, so a test can assert *which* mount
    /// was selected by its reported free space.
    fn a_mount_at(mount_point: &str, available_gb: u64) -> MountUsage {
        MountUsage {
            mount_point: PathBuf::from(mount_point),
            available_bytes: available_gb * 1_000_000_000,
            total_bytes: 500 * 1_000_000_000,
        }
    }

    #[test]
    fn single_root_mount_matches_any_absolute_path() {
        // Given a host with only the root filesystem mounted
        let mounts = vec![a_mount_at("/", 10)];

        // When selecting the mount for a deeply nested project directory
        let selected = select_mount_for_path(&mounts, Path::new("/home/dev/repos"));

        // Then the root mount is chosen
        assert_eq!(
            selected.map(|m| m.mount_point.as_path()),
            Some(Path::new("/"))
        );
    }

    #[test]
    fn longest_prefix_mount_wins_over_root() {
        // Given the root filesystem and a dedicated `/home` filesystem, with distinct free space
        let mounts = vec![a_mount_at("/", 10), a_mount_at("/home", 42)];

        // When selecting the mount for a directory under /home
        let selected = select_mount_for_path(&mounts, Path::new("/home/dev/repos"));

        // Then the more specific `/home` mount is chosen (its 42 GB free, not root's 10 GB)
        assert_eq!(selected.map(|m| m.available_bytes), Some(42_000_000_000));
    }

    #[test]
    fn partial_path_component_is_not_a_prefix() {
        // Given a mount whose final component is a partial match of the target's component
        let mounts = vec![a_mount_at("/ho", 10)];

        // When selecting the mount for `/home/dev`
        let selected = select_mount_for_path(&mounts, Path::new("/home/dev"));

        // Then `/ho` is not treated as a prefix of `/home/dev`
        assert!(selected.is_none());
    }

    #[test]
    fn returns_none_when_no_mount_is_a_prefix() {
        // Given mounts that are unrelated to the target directory
        let mounts = vec![a_mount_at("/mnt/data", 10), a_mount_at("/srv", 20)];

        // When selecting the mount for `/home/dev/repos`
        let selected = select_mount_for_path(&mounts, Path::new("/home/dev/repos"));

        // Then no mount matches
        assert!(selected.is_none());
    }
}

//! Multi-host daemon identity, discoverability, and routing.

/// Stable identifier for a daemon instance in a shared LiveKit common room (from config).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DaemonInstanceId(pub String);

/// One row for UI / API: which daemon can run a session and how it is labeled.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EligibleDaemonInfo {
    pub instance_id: DaemonInstanceId,
    pub label: String,
}

/// Source of eligible daemons for explicit selection before StartSession / ConnectSession / ResumeSession.
pub trait EligibleDaemonSource: Send + Sync {
    fn list_eligible_daemons(&self) -> Vec<EligibleDaemonInfo>;
}

/// Returns the local machine’s default daemon instance id (short hostname when available).
pub fn local_daemon_instance_id() -> DaemonInstanceId {
    DaemonInstanceId(local_hostname_or_local())
}

fn local_hostname_or_local() -> String {
    #[cfg(unix)]
    {
        let mut buf = [0u8; 256];
        let rc = unsafe { libc::gethostname(buf.as_mut_ptr() as *mut libc::c_char, buf.len()) };
        if rc != 0 {
            log::debug!("local_hostname_or_local: gethostname failed rc={}", rc);
            "local".to_string()
        } else {
            let len = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
            let s = std::str::from_utf8(&buf[..len])
                .map(|s| s.trim().to_string())
                .unwrap_or_else(|_| "local".to_string());
            if s.is_empty() {
                "local".to_string()
            } else {
                s
            }
        }
    }
    #[cfg(not(unix))]
    {
        "local".to_string()
    }
}

/// One eligible row for this process until remote discovery lists peers via LiveKit / API.
pub fn local_eligible_daemon_entry() -> EligibleDaemonInfo {
    let instance_id = local_daemon_instance_id();
    let label = format!("{} (this daemon)", instance_id.0);
    log::debug!(
        "local_eligible_daemon_entry: instance_id={} label={}",
        instance_id.0,
        label
    );
    EligibleDaemonInfo { instance_id, label }
}

/// Stub lists the local daemon until remote registry / presence is wired.
pub struct StubEligibleDaemonSource;

impl EligibleDaemonSource for StubEligibleDaemonSource {
    fn list_eligible_daemons(&self) -> Vec<EligibleDaemonInfo> {
        let entry = local_eligible_daemon_entry();
        log::info!(
            "StubEligibleDaemonSource: listing local daemon instance_id={}",
            entry.instance_id.0
        );
        vec![entry]
    }
}

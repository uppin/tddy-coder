use super::DaemonSessionHost;

impl DaemonSessionHost {
    /// Record RPC activity in the idle-timeout tracker, if one is attached.
    pub(crate) fn record_rpc_activity(&self) {
        self.rpc_activity.record();
    }
}

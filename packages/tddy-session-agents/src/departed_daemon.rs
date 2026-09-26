use tddy_rpc::Status;

pub fn refuse_departed_daemon(
    daemon_instance_id: &str,
    eligible: Vec<String>,
) -> Result<(), Status> {
    if eligible
        .iter()
        .any(|candidate| candidate == daemon_instance_id)
    {
        return Ok(());
    }
    Err(Status::unavailable(format!(
        "daemon '{daemon_instance_id}' has left the common room, so the agents it owns on this \
             session cannot be reached; the rest of the roster is unaffected"
    )))
}

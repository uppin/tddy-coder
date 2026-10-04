pub(crate) fn peer_has_no_such_session(status: &Status) -> bool {
    matches!(
        status.code,
        tddy_rpc::Code::FailedPrecondition | tddy_rpc::Code::NotFound
    )
}
use tddy_rpc::Status;

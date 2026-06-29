#[cfg(test)]
mod tests {
    use tddy_task::{ChannelKind, TaskChannel};

    #[test]
    fn pty_channel_kind_is_pty_and_accepts_input() {
        // Given / When
        let (ch, _stdin) = TaskChannel::pty("0", "pty");

        // Then
        assert_eq!(ch.kind, ChannelKind::Pty);
        assert!(ch.accepts_input());
    }
}

use russh::ChannelMsg;

#[derive(Default)]
pub(crate) struct PtyReadState {
    pending: Vec<u8>,
    offset: usize,
    pub(crate) eof: bool,
    pub(crate) closed: bool,
    pub(crate) exit_code: Option<u32>,
    pub(crate) exit_signal: Option<String>,
}

impl PtyReadState {
    pub(crate) fn drain(&mut self, buffer: &mut [u8]) -> usize {
        let count = buffer.len().min(self.pending.len() - self.offset);
        buffer[..count].copy_from_slice(&self.pending[self.offset..self.offset + count]);
        self.offset += count;
        if self.offset == self.pending.len() {
            self.pending.clear();
            self.offset = 0;
        }
        count
    }

    pub(crate) fn accept(&mut self, message: Option<ChannelMsg>) {
        match message {
            Some(ChannelMsg::Data { data } | ChannelMsg::ExtendedData { data, ext: 1 })
                if !self.eof =>
            {
                // RFC 4254: EOF ends inbound data; a late-exit monitor must not buffer protocol-invalid output.
                self.pending.extend_from_slice(&data);
            }
            Some(ChannelMsg::ExitStatus { exit_status }) => self.exit_code = Some(exit_status),
            Some(ChannelMsg::ExitSignal { signal_name, .. }) => {
                self.exit_signal = Some(format!("{signal_name:?}"));
            }
            Some(ChannelMsg::Eof) => self.eof = true,
            Some(ChannelMsg::Close) | None => {
                self.eof = true;
                self.closed = true;
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_packet_tail_across_smaller_reads() {
        let original: Vec<u8> = (0..32768).map(|i| (i % 251) as u8).collect();
        let mut state = PtyReadState::default();
        state.accept(Some(ChannelMsg::Data {
            data: original.clone().into(),
        }));
        let mut output = Vec::new();
        let mut buffer = [0; 16384];
        for _ in 0..2 {
            let count = state.drain(&mut buffer);
            assert_eq!(count, buffer.len());
            output.extend_from_slice(&buffer[..count]);
        }
        assert_eq!(output, original);
        assert_eq!(state.drain(&mut buffer), 0);
    }

    #[test]
    fn preserves_host_exit_evidence_before_and_after_eof() {
        for exit_before_eof in [true, false] {
            let mut state = PtyReadState::default();
            if !exit_before_eof {
                state.accept(Some(ChannelMsg::Eof));
            }
            state.accept(Some(ChannelMsg::ExitStatus { exit_status: 7 }));
            if exit_before_eof {
                state.accept(Some(ChannelMsg::Eof));
            }
            state.accept(Some(ChannelMsg::Close));
            assert_eq!(state.exit_code, Some(7));
            assert!(state.closed);
        }
    }

    #[test]
    fn channel_loss_and_eof_are_not_process_exit_evidence() {
        for message in [None, Some(ChannelMsg::Eof), Some(ChannelMsg::Close)] {
            let mut state = PtyReadState::default();
            state.accept(message);
            assert!(state.eof);
            assert_eq!(state.exit_code, None);
            assert_eq!(state.exit_signal, None);
        }
    }

    #[test]
    fn signal_and_extended_output_survive_channel_close() {
        let mut state = PtyReadState::default();
        state.accept(Some(ChannelMsg::ExtendedData {
            data: b"error".to_vec().into(),
            ext: 1,
        }));
        state.accept(Some(ChannelMsg::ExitSignal {
            signal_name: russh::Sig::TERM,
            core_dumped: false,
            error_message: String::new(),
            lang_tag: String::new(),
        }));
        state.accept(None);
        let mut buffer = [0; 8];
        assert_eq!(state.drain(&mut buffer), 5);
        assert_eq!(&buffer[..5], b"error");
        assert!(state.exit_signal.is_some());
    }
    #[test]
    fn late_data_after_eof_is_not_buffered_while_monitoring_exit() {
        let mut state = PtyReadState::default();
        state.accept(Some(ChannelMsg::Eof));
        for _ in 0..8 {
            state.accept(Some(ChannelMsg::Data {
                data: vec![b'x'; 32768].into(),
            }));
        }
        assert!(state.pending.is_empty());
        state.accept(Some(ChannelMsg::ExitStatus { exit_status: 3 }));
        assert_eq!(state.exit_code, Some(3));
    }
}

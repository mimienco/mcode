use russh::ChannelMsg;

use crate::{EXEC_STREAM_MAX_BYTES, SshError, session::ExecOutput};

#[derive(Default)]
pub(crate) struct ExecCollector {
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    exit_code: Option<u32>,
}

impl ExecCollector {
    pub(crate) fn accept(&mut self, message: ChannelMsg) -> Result<bool, SshError> {
        match message {
            ChannelMsg::Data { data } => append(&mut self.stdout, &data)?,
            ChannelMsg::ExtendedData { data, ext: 1 } => append(&mut self.stderr, &data)?,
            ChannelMsg::ExitStatus { exit_status } => self.exit_code = Some(exit_status),
            ChannelMsg::Close => return Ok(true),
            _ => {}
        }
        Ok(false)
    }

    pub(crate) fn finish(self) -> Result<ExecOutput, SshError> {
        Ok(ExecOutput {
            stdout: self.stdout,
            stderr: self.stderr,
            exit_code: self.exit_code.ok_or(SshError::ChannelClosed)?,
        })
    }
}

fn append(stream: &mut Vec<u8>, data: &[u8]) -> Result<(), SshError> {
    if data.len() > EXEC_STREAM_MAX_BYTES.saturating_sub(stream.len()) {
        return Err(SshError::Protocol(
            "exec output exceeds the stream limit".into(),
        ));
    }
    stream.extend_from_slice(data);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closed_channel_without_exit_status_is_not_a_completed_command() {
        let mut collector = ExecCollector::default();
        assert!(!collector.accept(ChannelMsg::Eof).unwrap());
        assert!(collector.accept(ChannelMsg::Close).unwrap());
        assert_eq!(collector.finish().unwrap_err().code(), "ssh_channel_closed");
    }

    #[test]
    fn collects_separate_streams_even_after_exit_status() {
        let mut collector = ExecCollector::default();
        collector
            .accept(ChannelMsg::ExitStatus { exit_status: 7 })
            .unwrap();
        collector
            .accept(ChannelMsg::Data {
                data: b"out".to_vec().into(),
            })
            .unwrap();
        collector
            .accept(ChannelMsg::ExtendedData {
                data: b"err".to_vec().into(),
                ext: 1,
            })
            .unwrap();
        let output = collector.finish().unwrap();
        assert_eq!(output.exit_code, 7);
        assert_eq!(output.stdout, b"out");
        assert_eq!(output.stderr, b"err");
    }

    #[test]
    fn accepts_exact_limit_but_rejects_next_byte_on_either_stream() {
        for extended in [false, true] {
            let mut collector = ExecCollector::default();
            let data = vec![b'x'; EXEC_STREAM_MAX_BYTES].into();
            let message = if extended {
                ChannelMsg::ExtendedData { data, ext: 1 }
            } else {
                ChannelMsg::Data { data }
            };
            collector.accept(message).unwrap();
            let data = vec![b'y'].into();
            let message = if extended {
                ChannelMsg::ExtendedData { data, ext: 1 }
            } else {
                ChannelMsg::Data { data }
            };
            assert_eq!(
                collector.accept(message).unwrap_err().code(),
                "ssh_protocol"
            );
            assert!(collector.stdout.len() <= EXEC_STREAM_MAX_BYTES);
            assert!(collector.stderr.len() <= EXEC_STREAM_MAX_BYTES);
        }
    }

    #[test]
    fn oversized_packet_is_rejected_before_copying() {
        let mut collector = ExecCollector::default();
        let error = collector.accept(ChannelMsg::Data {
            data: vec![0; EXEC_STREAM_MAX_BYTES + 1].into(),
        });
        assert!(error.is_err());
        assert!(collector.stdout.is_empty());
    }
}

//! Preserve late owning-host exit evidence after an output stream reaches EOF.
use mcode_ssh::session::PtyWaitResult;

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Decision {
    Wait,
    Unverifiable { finished: bool },
    Exited(i32),
    Stop,
}

#[derive(Default)]
pub(super) struct ExitMonitor {
    reported_unverifiable: bool,
}

impl ExitMonitor {
    pub(super) fn observe(
        &mut self,
        current: bool,
        evidence: &PtyWaitResult,
        closed: bool,
        grace_elapsed: bool,
    ) -> Decision {
        if !current {
            return Decision::Stop;
        }
        if let Some(code) = super::confirmed_ssh_exit_code(evidence) {
            return Decision::Exited(code);
        }
        if closed || grace_elapsed {
            if !self.reported_unverifiable {
                self.reported_unverifiable = true;
                return Decision::Unverifiable { finished: closed };
            }
            if closed {
                return Decision::Stop;
            }
        }
        Decision::Wait
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn no_evidence() -> PtyWaitResult {
        PtyWaitResult {
            exit_code: None,
            exit_signal: None,
        }
    }

    #[test]
    fn late_host_exit_after_unverifiable_is_still_delivered() {
        let mut monitor = ExitMonitor::default();
        assert_eq!(
            monitor.observe(true, &no_evidence(), false, false),
            Decision::Wait
        );
        assert_eq!(
            monitor.observe(true, &no_evidence(), false, true),
            Decision::Unverifiable { finished: false }
        );
        assert_eq!(
            monitor.observe(true, &no_evidence(), false, true),
            Decision::Wait
        );
        assert_eq!(
            monitor.observe(
                true,
                &PtyWaitResult {
                    exit_code: Some(7),
                    exit_signal: None
                },
                true,
                true
            ),
            Decision::Exited(7)
        );
    }

    #[test]
    fn actual_transport_loss_never_becomes_an_exit() {
        let mut monitor = ExitMonitor::default();
        assert_eq!(
            monitor.observe(true, &no_evidence(), true, false),
            Decision::Unverifiable { finished: true }
        );
        assert_eq!(
            monitor.observe(true, &no_evidence(), true, true),
            Decision::Stop
        );
    }

    #[test]
    fn removed_or_replaced_identity_cancels_monitor_even_with_exit_evidence() {
        let mut monitor = ExitMonitor::default();
        assert_eq!(
            monitor.observe(
                false,
                &PtyWaitResult {
                    exit_code: Some(0),
                    exit_signal: None
                },
                true,
                true
            ),
            Decision::Stop
        );
    }
}

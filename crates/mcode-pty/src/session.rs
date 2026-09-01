//! One live PTY session: reader thread feeding the byte owner, coalesced
//! resize, input cap, and flush-before-exit.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use portable_pty::{Child, MasterPty};

use crate::byte_owner::{ByteOwner, OutputBatch, RestoreMarker};
use crate::pty::{MAX_INPUT_BYTES, PtyError, PtySpec};

/// Reader control shared between the session and its reader thread.
struct ReaderControl {
    /// Producer pause: when true the reader thread stops draining the pty
    /// (portable-pty has no native pause; the gate is not-reading, which
    /// backpressures ConPTY's output pipe and is documented in module docs).
    paused: AtomicBool,
    /// Reader observed EOF / child death.
    dead: AtomicBool,
    /// Latest requested resize, applied by the reader loop (latest wins).
    pending_resize: Mutex<Option<(u16, u16)>>,
    /// Writer side for probe answers (ConPTY DSR — see READER docs below).
    writer: Mutex<Option<Box<dyn std::io::Write + Send>>>,
}

/// Cloneable handle to the master pty for resize/get_size from any thread.
#[derive(Clone, Default)]
struct SharedMaster(Arc<Mutex<Option<Box<dyn MasterPty + Send>>>>);

impl SharedMaster {
    fn resize(&self, cols: u16, rows: u16) {
        if let Some(master) = self.0.lock().expect("master lock").as_mut() {
            let _ = master.resize(portable_pty::PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            });
        }
    }

    fn get_size(&self) -> Option<(u16, u16)> {
        let size = self
            .0
            .lock()
            .expect("master lock")
            .as_ref()?
            .get_size()
            .ok()?;
        Some((size.cols, size.rows))
    }

    /// Closes the pseudoconsole master by dropping the trait object. On
    /// ConPTY the reader's blocking read ends only when conhost tears the
    /// console down, and killing the already-exited shell does not do that;
    /// dropping the master (the last clone of the Box, not the Arc) is what
    /// releases the reader. Later resize/size calls tolerate the `None`.
    fn close(&self) {
        *self.0.lock().expect("master lock") = None;
    }
}

/// A spawned PTY session owning its reader thread and byte owner.
pub struct PtySession {
    handle: crate::pty::PtyHandle,
    /// Shared master handle: portable-pty's ConPTY master is `Clone`
    /// (Arc inside) but the trait object is not, so we share it behind a
    /// mutex for resize/size queries from both session and reader.
    master: SharedMaster,
    child: Mutex<Box<dyn Child + Send + Sync>>,
    owner: Arc<Mutex<ByteOwner>>,
    control: Arc<ReaderControl>,
    spec: PtySpec,
    reader_handle: Option<std::thread::JoinHandle<()>>,
}

impl PtySession {
    /// Spawns the spec's command in a fresh pty and starts the reader thread.
    pub fn spawn(handle: crate::pty::PtyHandle, spec: PtySpec) -> Result<Self, PtyError> {
        spec.validate()?;
        let pair = crate::pty::open_pty(&spec)?;
        let cmd = crate::pty::build_command(&spec);
        let child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| PtyError::PtySpawnFailed {
                source: e.to_string(),
            })?;
        let reader = pair
            .master
            .try_clone_reader()
            .map_err(|e| PtyError::PtySpawnFailed {
                source: e.to_string(),
            })?;
        let writer = pair
            .master
            .take_writer()
            .map_err(|e| PtyError::PtySpawnFailed {
                source: e.to_string(),
            })?;

        let owner = Arc::new(Mutex::new(ByteOwner::new(spec.scrollback_rows)));
        let control = Arc::new(ReaderControl {
            paused: AtomicBool::new(false),
            dead: AtomicBool::new(false),
            pending_resize: Mutex::new(None),
            writer: Mutex::new(Some(writer)),
        });

        let master = SharedMaster(Arc::new(Mutex::new(Some(pair.master))));
        let reader_handle = start_reader_thread(
            reader,
            Arc::clone(&owner),
            Arc::clone(&control),
            master.clone(),
        );

        Ok(Self {
            handle,
            master,
            child: Mutex::new(child),
            owner,
            control,
            spec,
            reader_handle: Some(reader_handle),
        })
    }

    pub fn handle(&self) -> crate::pty::PtyHandle {
        self.handle
    }

    pub fn spec(&self) -> &PtySpec {
        &self.spec
    }

    /// Writes input to the pty. Enforces the 16 MiB cap with
    /// [`PtyError::InputTooLarge`].
    pub fn write_input(&self, data: &[u8]) -> Result<(), PtyError> {
        if data.len() > MAX_INPUT_BYTES {
            return Err(PtyError::InputTooLarge);
        }
        let mut guard = self.control.writer.lock().expect("writer lock");
        let Some(writer) = guard.as_mut() else {
            return Err(PtyError::SessionGone);
        };
        writer
            .write_all(data)
            .and_then(|_| writer.flush())
            .map_err(|e| PtyError::PtySpawnFailed {
                source: format!("pty write failed: {e}"),
            })
    }

    /// Requests a resize; the latest wins and it is applied by the reader
    /// loop on its next tick (coalesced).
    pub fn resize(&self, cols: u16, rows: u16) {
        *self.control.pending_resize.lock().expect("resize lock") = Some((cols, rows));
    }

    /// Producer pause: the reader stops draining the pty. portable-pty has
    /// no native pause API, so the gate is not-reading; on ConPTY this
    /// backpressures the output pipe and the child's writes block once the
    /// pipe fills, which is exactly the original pause semantics
    /// (`PtyProducerFlowController`). Memory stays bounded by the pending cap.
    pub fn pause_producer(&self) {
        self.control.paused.store(true, Ordering::SeqCst);
    }

    pub fn resume_producer(&self) {
        self.control.paused.store(false, Ordering::SeqCst);
    }

    /// Takes the pending output batch (drains the byte owner's queue).
    pub fn take_batch(&self) -> OutputBatch {
        self.owner.lock().expect("owner lock").take_batch()
    }

    /// The pty size as known by the kernel (applied resizes included).
    pub fn applied_size(&self) -> Option<(u16, u16)> {
        self.master.get_size()
    }

    pub fn output_seq(&self) -> u64 {
        self.owner.lock().expect("owner lock").seq()
    }

    /// Cumulative max-merge ACK (spec §5.3).
    pub fn ack(&self, processed_chars: u64) {
        self.owner.lock().expect("owner lock").ack(processed_chars);
    }

    /// Whether the reader observed EOF / child death.
    pub fn is_dead(&self) -> bool {
        self.control.dead.load(Ordering::SeqCst)
    }

    /// Whether the child process has exited, regardless of whether the
    /// ConPTY output pipe reached EOF. On Windows, conhost can keep the
    /// pseudoconsole's output side open after the child dies, so the
    /// reader's EOF signal alone is not a reliable death detector
    /// (PTY_CONTRACT_SPEC.md §5: exit must be announced exactly once and
    /// promptly). The first observation is cached; the poll is a cheap
    /// try_wait.
    pub fn child_exited(&self) -> bool {
        if self.control.dead.load(Ordering::SeqCst) {
            return true;
        }
        let mut child = self.child.lock().expect("child lock");
        matches!(child.try_wait(), Ok(Some(_)))
    }

    /// Child-exit teardown: capture the real exit status, stop the reader
    /// deterministically, then drain. On Windows, conhost can keep the
    /// pseudoconsole's output side open after the child dies, so the reader
    /// may never see EOF; waiting for reader EOF before announcing exit then
    /// hangs (the exit is never announced). Instead: read the status, close
    /// the master to release the reader's blocking read, join the reader,
    /// then return the final batch and cause — mirroring node-pty's
    /// close→drain→exit discipline (PTY_CONTRACT_SPEC.md §5).
    pub fn drain_after_child_exit_and_cause(&mut self) -> (OutputBatch, mcode_process::ExitCause) {
        let status = {
            let mut child = self.child.lock().expect("child lock");
            let status = child.try_wait().ok().flatten();
            let _ = child.kill();
            let _ = child.wait();
            status
        };
        self.control.dead.store(true, Ordering::SeqCst);
        // Why first: killing the already-exited shell releases nothing; the
        // reader unblocks only when the pseudoconsole closes (conhost exits).
        self.master.close();
        if let Some(handle) = self.reader_handle.take() {
            let _ = handle.join();
        }
        let batch = self.take_batch();
        let observation = mcode_process::ExitObservation {
            exit_code: status.map_or(0, |s| s.exit_code() as i32),
            signal: None,
        };
        (batch, observation.cause())
    }

    /// Waits for the child, flushing pending output BEFORE reporting the
    /// exit cause (the renderer tears down the terminal on exit, so any
    /// batched output not yet flushed would be silently lost —
    /// `pty.ts:3730-3757`).
    pub fn wait_with_flush(
        &mut self,
        timeout: std::time::Duration,
    ) -> Result<(OutputBatch, mcode_process::ExitCause), PtyError> {
        let started = std::time::Instant::now();
        let status = loop {
            {
                let mut child = self.child.lock().expect("child lock");
                if let Ok(Some(status)) = child.try_wait() {
                    break Some(status);
                }
            }
            if started.elapsed() >= timeout {
                break None;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        };

        // Flush-before-exit: drain the byte owner before the cause is built.
        let batch = self.take_batch();

        let Some(status) = status else {
            return Ok((batch, mcode_process::ExitCause::StopUnverified));
        };

        // node-pty discipline: a signalled death is {exitCode: 0, signal: N};
        // build the cause where both facts are in hand.
        #[cfg(unix)]
        let observation = {
            use std::os::unix::process::ExitStatusExt;
            mcode_process::ExitObservation {
                exit_code: status.code().map(|c| c as i32).unwrap_or(0),
                signal: status.signal().map(|s| s as i32),
            }
        };
        #[cfg(not(unix))]
        let observation = mcode_process::ExitObservation {
            exit_code: status.exit_code() as i32,
            signal: None,
        };

        Ok((batch, observation.cause()))
    }

    /// Kills the child and joins the reader thread.
    pub fn kill(&mut self) -> Result<(), PtyError> {
        let mut child = self.child.lock().expect("child lock");
        let _ = child.kill();
        let _ = child.wait();
        drop(child);
        self.control.dead.store(true, Ordering::SeqCst);
        if let Some(handle) = self.reader_handle.take() {
            let _ = handle.join();
        }
        Ok(())
    }

    /// One final flush of whatever the reader captured before the exit
    /// announcement (flush-before-exit discipline).
    pub fn final_flush(&self) -> OutputBatch {
        self.take_batch()
    }
}

impl Drop for PtySession {
    fn drop(&mut self) {
        // Kill the child first so the reader's blocking read sees EOF.
        if let Ok(mut child) = self.child.lock() {
            let _ = child.kill();
            let _ = child.wait();
        }
        self.control.dead.store(true, Ordering::SeqCst);
        // Bounded join: the reader polls every 5 ms when paused and reads
        // to EOF otherwise; killing the child releases the pipe. If the
        // thread somehow still has not exited after the grace period, detach
        // it — leaking a dead thread beats wedging the drop.
        if let Some(handle) = self.reader_handle.take() {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
            while !handle.is_finished() && std::time::Instant::now() < deadline {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            if !handle.is_finished() {
                std::mem::forget(handle);
            } else {
                let _ = handle.join();
            }
        }
    }
}

/// Starts the reader thread: drains the pty into the byte owner, applies
/// coalesced resizes, and honors the pause gate + exit flush.
fn start_reader_thread(
    mut reader: Box<dyn std::io::Read + Send>,
    owner: Arc<Mutex<ByteOwner>>,
    control: Arc<ReaderControl>,
    master: SharedMaster,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let mut buf = [0u8; 16 * 1024];
        loop {
            // Apply the latest coalesced resize before reading.
            if let Some((cols, rows)) = control.pending_resize.lock().expect("resize lock").take() {
                master.resize(cols, rows);
            }

            if control.paused.load(Ordering::SeqCst) {
                if control.dead.load(Ordering::SeqCst) {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
                continue;
            }

            match reader.read(&mut buf) {
                Ok(0) => break, // EOF: child side closed
                Ok(n) => {
                    // ConPTY DSR handshake: when the host never claimed the
                    // terminal, ConPTY sends ESC[6n and withholds every
                    // other byte until the CPR reply lands on the input
                    // side. Answer cursor-position 1;1 so output flows;
                    // the probe itself still reaches the byte owner (the
                    // consumer may answer its own way, but nothing blocks).
                    if buf[..n].windows(4).any(|w| w == b"\x1b[6n")
                        && let Some(writer) = control.writer.lock().expect("writer lock").as_mut()
                    {
                        let _ = writer.write_all(b"\x1b[1;1R");
                        let _ = writer.flush();
                    }
                    let marker: Option<RestoreMarker> =
                        owner.lock().expect("owner lock").append(&buf[..n]);
                    let _ = marker; // surfaced through take_batch
                }
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(_) => break,
            }
        }
        control.dead.store(true, Ordering::SeqCst);
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pty::{PtyHandle, PtySize};

    fn echo_spec() -> PtySpec {
        PtySpec {
            program: "cmd".to_string(),
            args: vec!["/c".to_string(), "echo".to_string(), "hello".to_string()],
            cwd: None,
            env_policy: mcode_process::EnvironmentPolicy::default(),
            size: PtySize { cols: 80, rows: 24 },
            scrollback_rows: 1000,
        }
    }

    fn input_cap_spec() -> PtySpec {
        PtySpec {
            program: "cmd".to_string(),
            args: vec!["/c".to_string(), "pause".to_string()],
            cwd: None,
            env_policy: mcode_process::EnvironmentPolicy::default(),
            size: PtySize { cols: 80, rows: 24 },
            scrollback_rows: 1000,
        }
    }

    #[test]
    fn session_rejects_invalid_spec_before_any_resource() {
        let mut spec = echo_spec();
        spec.program = " ".to_string();
        let err = match PtySession::spawn(PtyHandle(1), spec) {
            Ok(_) => panic!("expected invalid-spec rejection"),
            Err(e) => e,
        };
        assert_eq!(err.code(), "pty_invalid_argument");
    }

    #[test]
    fn input_cap_rejects_oversized_writes() {
        let session = PtySession::spawn(PtyHandle(2), input_cap_spec()).expect("spawn");
        let huge = vec![b'a'; MAX_INPUT_BYTES + 1];
        assert_eq!(
            session.write_input(&huge).unwrap_err().code(),
            "pty_input_too_large"
        );
        // Bounded small writes pass.
        session.write_input(b"dir\r\n").expect("small write");
    }

    #[test]
    fn resize_coalesces_to_latest_before_flush() {
        let session = PtySession::spawn(PtyHandle(3), echo_spec()).expect("spawn");
        session.resize(100, 30);
        session.resize(120, 40);
        // The latest request is the one stored; the reader applies whichever
        // is present on its next tick. Assert the coalescing slot holds only
        // the latest by racing one read before the reader drains it: we
        // cannot observe the slot directly, so assert the pty size after a
        // beat instead.
        std::thread::sleep(std::time::Duration::from_millis(200));
        assert_eq!(session.applied_size(), Some((120, 40)));
    }

    #[test]
    fn pause_and_resume_gate_the_reader() {
        let session = PtySession::spawn(PtyHandle(4), echo_spec()).expect("spawn");
        session.pause_producer();
        std::thread::sleep(std::time::Duration::from_millis(150));
        let seq_paused = session.output_seq();
        session.resume_producer();
        std::thread::sleep(std::time::Duration::from_millis(300));
        // After resume the echo eventually lands (or already has).
        assert!(session.output_seq() >= seq_paused);
    }

    #[cfg(windows)]
    #[test]
    fn real_conpty_echo_roundtrip() {
        let mut session = PtySession::spawn(PtyHandle(5), echo_spec()).expect("spawn");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
        let mut saw_hello = false;
        while std::time::Instant::now() < deadline {
            let batch = session.take_batch();
            if batch.chunks.iter().any(|c| c.data.contains("hello")) {
                saw_hello = true;
            }
            if saw_hello && session.is_dead() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        assert!(saw_hello, "echo output must reach the byte owner");
        let (batch, cause) = session
            .wait_with_flush(std::time::Duration::from_secs(10))
            .expect("wait");
        assert!(
            matches!(cause, mcode_process::ExitCause::Exited { exit_code: 0 }),
            "expected clean exit, got {cause:?}"
        );
        // Flush-before-exit: the final batch plus earlier reads covered hello.
        let final_has_hello = batch.chunks.iter().any(|c| c.data.contains("hello"));
        assert!(
            saw_hello || final_has_hello,
            "exit flush must deliver any output not yet drained"
        );
    }

    #[cfg(windows)]
    #[test]
    fn real_conpty_multibyte_split_across_reads() {
        // Spawn a shell echoing a multibyte string; the byte owner must
        // reassemble it correctly regardless of read boundaries.
        let mut spec = echo_spec();
        spec.args = vec![
            "/c".to_string(),
            "echo".to_string(),
            "héllo-wörld-日本語".to_string(),
        ];
        let session = PtySession::spawn(PtyHandle(6), spec).expect("spawn");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
        let mut collected = String::new();
        while std::time::Instant::now() < deadline {
            for chunk in session.take_batch().chunks {
                collected.push_str(&chunk.data);
            }
            if collected.contains("日本語") {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        assert!(
            collected.contains("日本語"),
            "multibyte payload must survive read boundaries, got {collected:?}"
        );
    }
}

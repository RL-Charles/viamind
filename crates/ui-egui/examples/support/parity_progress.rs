//! Independently timed, fixed-size observations of the diagnostic callback only.
use std::{
    fs::OpenOptions,
    io::Write,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc::{self, SyncSender},
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

use serde::Serialize;

const OUTPUT_LIMIT: usize = 8192;
const STACK_BYTES: usize = 64 * 1024;
// Precharge the fixed state, bounded path/channel allowance, requested worker stack,
// and full streamed-output allowance in the parent trace's aggregate metadata budget.
pub const BUDGET_BYTES: usize = 512 + 8192 + STACK_BYTES + OUTPUT_LIMIT;

#[derive(Clone, Copy)]
#[repr(u64)]
pub enum Phase {
    Startup,
    Budget,
    Extraction,
    Admission,
    Comparison,
    Drawing,
    TraceDrawing,
    Submission,
    Repaint,
    Returned,
    Flushing,
}

#[derive(Default)]
pub struct Progress {
    generation: AtomicU64,
    entries: AtomicU64,
    exits: AtomicU64,
    step: AtomicU64,
    settle: AtomicU64,
    phase: AtomicU64,
    prepared: AtomicU64,
    submitted: AtomicU64,
    arrivals: AtomicU64,
    pending_sequence: AtomicU64,
    frame: AtomicU64,
    pass: AtomicU64,
    terminal: AtomicU64,
}

pub struct Callback(Arc<Progress>);
impl Drop for Callback {
    fn drop(&mut self) {
        self.0.phase(Phase::Returned);
        self.0.exits.fetch_add(1, Ordering::SeqCst);
        self.0.generation.fetch_add(1, Ordering::SeqCst);
    }
}

impl Progress {
    pub fn enter(self: &Arc<Self>, step: usize, settle: usize) -> Callback {
        self.generation.fetch_add(1, Ordering::SeqCst);
        self.entries.fetch_add(1, Ordering::SeqCst);
        self.step.store(step as u64, Ordering::SeqCst);
        self.settle.store(settle as u64, Ordering::SeqCst);
        self.phase(Phase::Budget);
        Callback(Arc::clone(self))
    }

    pub fn phase(&self, phase: Phase) {
        self.phase.store(phase as u64, Ordering::SeqCst);
    }

    pub fn position(&self, step: usize, settle: usize) {
        self.step.store(step as u64, Ordering::SeqCst);
        self.settle.store(settle as u64, Ordering::SeqCst);
    }

    pub fn observed_frame(&self, frame: u64, pass: u64) {
        self.frame.store(frame, Ordering::SeqCst);
        self.pass.store(pass, Ordering::SeqCst);
    }

    pub fn prepared(&self, sequence: usize) {
        self.prepared.fetch_add(1, Ordering::SeqCst);
        self.pending_sequence.store(sequence as u64 + 1, Ordering::SeqCst);
    }

    pub fn submitted(&self) {
        self.submitted.fetch_add(1, Ordering::SeqCst);
    }

    pub fn arrived(&self) {
        self.arrivals.fetch_add(1, Ordering::SeqCst);
        self.pending_sequence.store(0, Ordering::SeqCst);
    }

    fn snapshot(&self, between: impl FnOnce()) -> Snapshot {
        let first = self.generation.load(Ordering::SeqCst);
        let mut snapshot = Snapshot {
            generation_before: first,
            generation_after: 0,
            aggregate_consistent: false,
            callback_entries: self.entries.load(Ordering::SeqCst),
            callback_exits: self.exits.load(Ordering::SeqCst),
            legacy_step: self.step.load(Ordering::SeqCst),
            settle_count: self.settle.load(Ordering::SeqCst),
            phase: self.phase.load(Ordering::SeqCst),
            requests_prepared: self.prepared.load(Ordering::SeqCst),
            requests_submitted: self.submitted.load(Ordering::SeqCst),
            arrivals: self.arrivals.load(Ordering::SeqCst),
            pending_sequence_plus_one: self.pending_sequence.load(Ordering::SeqCst),
            observed_frame: self.frame.load(Ordering::SeqCst),
            observed_pass: self.pass.load(Ordering::SeqCst),
            terminal: self.terminal.load(Ordering::SeqCst),
        };
        between();
        snapshot.generation_after = self.generation.load(Ordering::SeqCst);
        snapshot.aggregate_consistent = first == snapshot.generation_after && first.is_multiple_of(2);
        snapshot
    }
}

#[derive(Serialize)]
struct Snapshot {
    generation_before: u64,
    generation_after: u64,
    aggregate_consistent: bool,
    callback_entries: u64,
    callback_exits: u64,
    legacy_step: u64,
    settle_count: u64,
    phase: u64,
    requests_prepared: u64,
    requests_submitted: u64,
    arrivals: u64,
    pending_sequence_plus_one: u64,
    observed_frame: u64,
    observed_pass: u64,
    terminal: u64,
}

pub struct Controller {
    pub progress: Arc<Progress>,
    terminal: Option<SyncSender<()>>,
}
impl Controller {
    /// At most one bounded notification. No wait for the independently timed writer.
    pub fn terminal(&mut self, success: bool) -> bool {
        let Some(sender) = self.terminal.take() else { return false };
        self.progress.terminal.store(if success { 1 } else { 2 }, Ordering::SeqCst);
        let _ = sender.try_send(());
        true
    }
}

pub struct Worker(JoinHandle<Result<(), String>>);
impl Worker {
    /// Called only after the native event loop returns, never inside a UI callback.
    pub fn join(self) -> Result<(), String> {
        self.0.join().map_err(|_| "diagnostic progress worker panicked".to_owned())?
    }
}

#[derive(Serialize)]
struct Receipt {
    reason: &'static str,
    elapsed_seconds: f64,
    observations: Snapshot,
    phase_names: [&'static str; 11],
    terminal_names: [&'static str; 3],
    claim: &'static str,
}

pub fn start(path: PathBuf, deadline: Duration) -> Result<(Controller, Worker), String> {
    if size_of::<Progress>() > 512 || size_of::<Progress>() / size_of::<AtomicU64>() > 32 {
        return Err("diagnostic atomic progress state exceeds fixed budget".into());
    }
    if path.as_os_str().len() > 4096 || path.exists() {
        return Err("diagnostic progress output must be NEW and bounded".into());
    }
    let progress = Arc::new(Progress::default());
    progress.phase(Phase::Startup);
    let observed = Arc::clone(&progress);
    let (terminal, receiver) = mpsc::sync_channel(1);
    let handle = std::thread::Builder::new()
        .name("parity-progress".into())
        .stack_size(STACK_BYTES)
        .spawn(move || {
            let started = Instant::now();
            let reason = match receiver.recv_timeout(deadline) {
                Ok(()) => "terminal_notification",
                Err(mpsc::RecvTimeoutError::Timeout) => "independent_timeout",
                Err(mpsc::RecvTimeoutError::Disconnected) => "terminal_sender_disconnected",
            };
            let receipt = Receipt {
                reason,
                elapsed_seconds: started.elapsed().as_secs_f64(),
                observations: observed.snapshot(|| {}),
                phase_names: [
                    "startup", "budget", "extraction", "admission", "comparison", "drawing", "trace_drawing", "submission", "repaint", "returned",
                    "flushing",
                ],
                terminal_names: ["unfinished", "trace_finished_not_pixel_verdict", "trace_failure"],
                claim: "Fixed atomic phase observations only; inconsistent aggregates are hints. No backend cause, pixel parity or performance claim.",
            };
            let file = OpenOptions::new().write(true).create_new(true).open(path).map_err(bounded_error)?;
            let mut writer = Capped { inner: file, bytes: 0 };
            serde_json::to_writer_pretty(&mut writer, &receipt).map_err(bounded_error)?;
            writer.write_all(b"\n").map_err(bounded_error)?;
            writer.inner.sync_all().map_err(bounded_error)
        })
        .map_err(bounded_error)?;
    Ok((Controller { progress, terminal: Some(terminal) }, Worker(handle)))
}

fn bounded_error(error: impl std::fmt::Display) -> String {
    error.to_string().chars().take(256).collect()
}

struct Capped<W> {
    inner: W,
    bytes: usize,
}
impl<W: Write> Write for Capped<W> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let next = self.bytes.checked_add(bytes.len()).filter(|&n| n <= OUTPUT_LIMIT);
        let Some(next) = next else { return Err(std::io::Error::other("diagnostic progress receipt exceeds 8KiB")) };
        let written = self.inner.write(bytes)?;
        self.bytes = next - (bytes.len() - written);
        Ok(written)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn output() -> (tempfile::TempDir, PathBuf) {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("progress.json");
        (directory, path)
    }
    fn receipt(path: &std::path::Path) -> serde_json::Value {
        serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
    }

    #[test]
    fn independent_deadline_records_pending_capture_without_more_callbacks() {
        let (_directory, path) = output();
        let (controller, worker) = start(path.clone(), Duration::from_millis(20)).unwrap();
        {
            let _callback = controller.progress.enter(6, 6);
            controller.progress.prepared(6);
            controller.progress.submitted();
        }
        worker.join().unwrap();
        let r = receipt(&path);
        assert_eq!(r["reason"], "independent_timeout");
        assert_eq!(r["observations"]["callback_entries"], 1);
        assert_eq!(r["observations"]["callback_exits"], 1);
        assert_eq!(r["observations"]["pending_sequence_plus_one"], 7);
        assert_eq!(r["observations"]["arrivals"], 0);
        assert_eq!(r["observations"]["aggregate_consistent"], true);
    }

    #[test]
    fn active_callback_and_changed_generation_are_explicitly_inconsistent() {
        let (_directory, path) = output();
        let (controller, worker) = start(path.clone(), Duration::from_millis(20)).unwrap();
        let callback = controller.progress.enter(6, 6);
        controller.progress.phase(Phase::Drawing);
        worker.join().unwrap();
        let r = receipt(&path);
        assert_eq!(r["observations"]["callback_exits"], 0);
        assert_eq!(r["observations"]["phase"], Phase::Drawing as u64);
        assert_eq!(r["observations"]["aggregate_consistent"], false);
        drop(callback);
        let raced = controller.progress.snapshot(|| {
            let _next = controller.progress.enter(7, 0);
        });
        assert!(!raced.aggregate_consistent);
        assert_ne!(raced.generation_before, raced.generation_after);
    }

    #[test]
    fn terminal_notification_is_nonblocking_once_and_no_sequence_file_exists() {
        let (_directory, path) = output();
        let (mut controller, worker) = start(path.clone(), Duration::from_secs(10)).unwrap();
        assert!(!path.exists());
        assert!(controller.terminal(true));
        assert!(!controller.terminal(false));
        worker.join().unwrap();
        let r = receipt(&path);
        assert_eq!(r["reason"], "terminal_notification");
        assert_eq!(r["observations"]["terminal"], 1);
        assert!(std::fs::metadata(path).unwrap().len() <= OUTPUT_LIMIT as u64);
    }

    #[test]
    fn disconnected_sender_and_new_path_race_do_not_overwrite() {
        let (_directory, path) = output();
        let (controller, worker) = start(path.clone(), Duration::from_secs(10)).unwrap();
        std::fs::write(&path, b"existing proof").unwrap();
        drop(controller);
        assert!(worker.join().is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"existing proof");
        assert!(start(path, Duration::ZERO).is_err());
        let (_directory, path) = output();
        let (controller, worker) = start(path.clone(), Duration::from_secs(10)).unwrap();
        drop(controller);
        worker.join().unwrap();
        assert_eq!(receipt(&path)["reason"], "terminal_sender_disconnected");
    }

    #[test]
    fn streamed_output_refuses_before_exceeding_reserved_budget() {
        let mut writer = Capped { inner: Vec::new(), bytes: 0 };
        assert!(writer.write_all(&[0; OUTPUT_LIMIT + 1]).is_err());
        assert!(writer.inner.is_empty());
        writer.write_all(&[1; OUTPUT_LIMIT]).unwrap();
        assert!(writer.write_all(&[2]).is_err());
        assert_eq!(writer.inner.len(), OUTPUT_LIMIT);
    }
}

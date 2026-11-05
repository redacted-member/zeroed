//! How far the running command has got, and whether the user asked it to
//! stop. One command runs at a time, so this is kept for the whole process:
//! the host reads it and raises the stop request from another thread while
//! the command runs, without waiting for the kernel.
//!
//! [`crate::Kernel::call`] starts and ends every command. Long operations
//! report their progress with [`report`] and call [`check`] (or
//! [`stopped`] inside parallel work) often enough to stop within a moment;
//! a stopped command fails with [`CoreError::Cancelled`] and, like any
//! failed command, leaves the document as it was.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use crate::error::CoreError;

/// Whether a command is running.
static RUNNING: AtomicBool = AtomicBool::new(false);
/// Whether the running command was asked to stop.
static STOP: AtomicBool = AtomicBool::new(false);
/// How far it has got, 0 to 1, as the bits of an `f64`; zero until the
/// command reports.
static DONE: AtomicU64 = AtomicU64::new(0);

/// Marks a command as started: nothing done yet, no stop asked for.
pub fn begin() {
    DONE.store(0f64.to_bits(), Ordering::SeqCst);
    STOP.store(false, Ordering::SeqCst);
    RUNNING.store(true, Ordering::SeqCst);
}

/// Marks the command as finished; a stop asked for too late is dropped.
pub fn end() {
    RUNNING.store(false, Ordering::SeqCst);
    STOP.store(false, Ordering::SeqCst);
    DONE.store(0f64.to_bits(), Ordering::SeqCst);
}

/// Reports how far the running command has got, 0 to 1.
pub fn report(fraction: f64) {
    if fraction.is_finite() {
        DONE.store(fraction.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
    }
}

/// Reports `done` of `total` steps.
pub fn report_steps(done: usize, total: usize) {
    if total > 0 {
        report(done as f64 / total as f64);
    }
}

/// Asks the running command to stop. Answers whether one was running.
pub fn request_stop() -> bool {
    let running = RUNNING.load(Ordering::SeqCst);
    if running {
        STOP.store(true, Ordering::SeqCst);
    }
    running
}

/// Whether the running command was asked to stop.
pub fn stopped() -> bool {
    STOP.load(Ordering::Relaxed)
}

/// Fails with [`CoreError::Cancelled`] once the command was asked to stop.
///
/// # Errors
/// [`CoreError::Cancelled`] after [`request_stop`].
pub fn check() -> Result<(), CoreError> {
    if stopped() {
        Err(CoreError::Cancelled)
    } else {
        Ok(())
    }
}

/// What a host shows while a command runs.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub struct Status {
    /// Whether a command is running.
    pub running: bool,
    /// How far it has got, 0 to 1; 0 when it does not report.
    pub progress: f64,
}

/// Whether a command is running and how far it has got.
pub fn status() -> Status {
    Status {
        running: RUNNING.load(Ordering::SeqCst),
        progress: f64::from_bits(DONE.load(Ordering::Relaxed)),
    }
}

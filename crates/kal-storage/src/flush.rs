//! Single-flight background-write bookkeeping for the wasm snapshot store.
//!
//! IndexedDB is asynchronous while the storage API is synchronous, so the whole
//! snapshot is captured in memory and written by a background task. Exactly one
//! task may run at a time — concurrent writes could land out of order and leave
//! an older snapshot behind — and mutations that arrive while it is awaiting a
//! write must schedule another pass instead of being merged into a write that
//! already captured the older state.
//!
//! The protocol is:
//!
//! 1. [`WriteGate::claim`] — a mutation asks for a flush. `true` means "spawn
//!    the task"; mutations while a task is running get `false` and are folded
//!    into that task's next pass.
//! 2. [`WriteGate::begin`] — the task starts a pass, clearing the dirty mark
//!    **before** it captures state, so anything mutated during the await is
//!    still visible to [`WriteGate::finish`].
//! 3. [`WriteGate::finish`] — the write completed; `true` means the state
//!    changed while it was in flight, so the task loops and writes again.
//!    [`WriteGate::release`] is the exit for a failed write.
//!
//! Clearing the mark only after the write (the previous behaviour) lost every
//! mutation made during the await: no new task was spawned because the mark was
//! still set, and the finishing task then cleared it.

use std::cell::Cell;

/// Dirty/running marks shared by the database and its one flush task.
pub(crate) struct WriteGate {
    dirty: Cell<bool>,
    running: Cell<bool>,
}

impl WriteGate {
    pub(crate) fn new() -> Self {
        Self {
            dirty: Cell::new(false),
            running: Cell::new(false),
        }
    }

    /// A mutation asks for a flush; `true` means the caller must spawn the task.
    pub(crate) fn claim(&self) -> bool {
        self.dirty.set(true);
        !self.running.replace(true)
    }

    /// Start a pass: clear the dirty mark before capturing the snapshot.
    pub(crate) fn begin(&self) {
        self.dirty.set(false);
    }

    /// End a successful pass; `true` means another pass is needed. When nothing
    /// changed during the write, the gate becomes claimable again.
    pub(crate) fn finish(&self) -> bool {
        if self.dirty.replace(false) {
            true
        } else {
            self.running.set(false);
            false
        }
    }

    /// The write failed and the task is exiting without a successful pass.
    pub(crate) fn release(&self) {
        self.running.set(false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A mutation during the awaited write must produce a second pass. If the
    /// mark were cleared after the write, that mutation would be forgotten and
    /// the browser would keep the older snapshot.
    #[test]
    fn mutations_during_a_write_ask_for_another_pass() {
        let gate = WriteGate::new();
        assert!(gate.claim(), "the first mutation spawns a flush");

        gate.begin();
        // The write is in flight and another mutation lands.
        assert!(!gate.claim(), "no second task while one is in flight");
        assert!(gate.finish(), "that mutation needs another pass");

        gate.begin();
        assert!(!gate.finish(), "quiescent: the loop can stop");
        assert!(gate.claim(), "a later mutation starts a fresh flush");
    }

    /// A failed write must not wedge the queue: the gate is released so the
    /// next mutation spawns a fresh task rather than joining one that exited.
    #[test]
    fn a_failed_write_leaves_the_queue_ready_for_the_next_mutation() {
        let gate = WriteGate::new();
        assert!(gate.claim());
        gate.begin();
        gate.release();
        assert!(gate.claim(), "the next mutation may start a new flush");
    }

    /// A mutation that lands between two passes is still followed by a write of
    /// the latest state, and the gate only goes quiet once nothing is pending.
    #[test]
    fn a_mutation_between_passes_extends_the_loop() {
        let gate = WriteGate::new();
        assert!(gate.claim());
        gate.begin();
        gate.claim();
        assert!(gate.finish(), "first pass asks for a second");

        gate.begin();
        assert!(!gate.finish(), "second pass covers the pending mutation");
        assert!(gate.claim());
    }
}

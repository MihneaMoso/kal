//! Startup tracing for the live sync path.
//!
//! iroh, iroh-gossip and distributed-topic-tracker emit detailed DEBUG
//! bootstrap traces, but the app installs no subscriber, so on a real device
//! they vanish. This installer routes them to a small rotating file next to
//! the calendar DB:
//!   - Android: `<files>/kal/sync-trace.log` — read it with
//!     `adb shell run-as com.kal.calendar cat files/kal/sync-trace.log`
//!   - Desktop: `~/.local/share/kal/sync-trace.log`
//!
//! The file is truncated on every app start so it reflects the latest run.
//! Everything is best-effort: any failure just means no trace file.

use std::path::PathBuf;

/// Install the tracing subscriber (idempotent; first caller wins). Call once
/// at startup, before any sync transport is built.
pub fn init_trace_log() {
    let Some(path) = trace_path() else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let file = match std::fs::File::create(&path) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("kal: trace log unavailable at {path:?}: {e}");
            return;
        }
    };
    eprintln!("kal: writing sync trace to {path:?}");
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_ansi(false)
        .with_target(true)
        .with_thread_names(true)
        .with_writer(file)
        .try_init();
}

fn trace_path() -> Option<PathBuf> {
    crate::app_data_dir().map(|d| d.join("kal").join("sync-trace.log"))
}

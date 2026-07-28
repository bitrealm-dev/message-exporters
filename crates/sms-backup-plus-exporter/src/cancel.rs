//! Cooperative cancellation for long-running convert loops.

use anyhow::Result;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Shared cancel flag for cooperative cancellation (GUI Cancel button).
pub type CancelFlag = Arc<AtomicBool>;

/// Whether cancel has been requested.
pub fn is_cancelled(cancel: Option<&CancelFlag>) -> bool {
    cancel
        .map(|flag| flag.load(Ordering::Relaxed))
        .unwrap_or(false)
}

/// Bail if cancel was requested.
pub(crate) fn check_cancel(cancel: Option<&CancelFlag>) -> Result<()> {
    if is_cancelled(cancel) {
        anyhow::bail!("cancelled");
    }
    Ok(())
}

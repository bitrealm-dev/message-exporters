//! Cooperative cancellation for in-process GUI runs.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::app::error::RuntimeError;

/// Shared cancel flag for cooperative cancellation (GUI Cancel button).
pub type CancelFlag = Arc<AtomicBool>;

/// Whether cancel has been requested.
pub fn is_cancelled(cancel: Option<&CancelFlag>) -> bool {
    cancel
        .map(|flag| flag.load(Ordering::Relaxed))
        .unwrap_or(false)
}

/// Bail if cancel was requested.
pub(crate) fn check_cancel(cancel: Option<&CancelFlag>) -> Result<(), RuntimeError> {
    if is_cancelled(cancel) {
        return Err(RuntimeError::InvalidOptions("cancelled".to_string()));
    }
    Ok(())
}

//! Cooperative cancellation for in-process GUI runs.

use crate::app::error::RuntimeError;
pub use message_exporters_core::{is_cancelled, CancelFlag};

/// Bail if cancel was requested.
pub(crate) fn check_cancel(cancel: Option<&CancelFlag>) -> Result<(), RuntimeError> {
    message_exporters_core::check_cancel(cancel)
        .map_err(|msg| RuntimeError::InvalidOptions(msg.to_string()))
}

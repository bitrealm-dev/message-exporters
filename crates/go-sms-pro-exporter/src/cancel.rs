//! Cooperative cancellation for long-running convert loops.

use anyhow::Result;
pub use message_exporters_core::{CancelFlag, is_cancelled};

/// Bail if cancel was requested.
pub(crate) fn check_cancel(cancel: Option<&CancelFlag>) -> Result<()> {
    message_exporters_core::check_cancel(cancel).map_err(anyhow::Error::msg)
}

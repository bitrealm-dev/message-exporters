//! Errors for the thin iMessage → mail archive exporter.

use std::{
    error::Error,
    fmt::{Display, Formatter, Result},
    io::Error as IoError,
    path::PathBuf,
};

use crabapple::error::BackupError;
use imessage_database::error::{message::MessageError, plist::PlistParseError, table::TableError};

/// Runtime failures while opening sources or writing mail archives.
#[derive(Debug)]
pub(crate) enum RuntimeError {
    InvalidOptions(String),
    DiskError(IoError),
    DatabaseError(TableError),
    MessageError(MessageError),
    BackupError(BackupError),
    FileNameError { path: PathBuf, reason: &'static str },
}

impl Display for RuntimeError {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result {
        match self {
            RuntimeError::InvalidOptions(why) => write!(fmt, "Invalid options!\n{why}"),
            RuntimeError::DiskError(why) => write!(fmt, "{why}"),
            RuntimeError::DatabaseError(why) => write!(fmt, "{why}"),
            RuntimeError::MessageError(why) => write!(fmt, "{why}"),
            RuntimeError::BackupError(why) => write!(fmt, "{why}"),
            RuntimeError::FileNameError { path, reason } => {
                write!(fmt, "Invalid file name at {}: {reason}", path.display())
            }
        }
    }
}

impl Error for RuntimeError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            RuntimeError::DiskError(why) => Some(why),
            RuntimeError::DatabaseError(why) => Some(why),
            RuntimeError::MessageError(why) => Some(why),
            RuntimeError::BackupError(why) => Some(why),
            RuntimeError::InvalidOptions(_) | RuntimeError::FileNameError { .. } => None,
        }
    }
}

impl From<TableError> for RuntimeError {
    fn from(err: TableError) -> Self {
        RuntimeError::DatabaseError(err)
    }
}

impl From<MessageError> for RuntimeError {
    fn from(err: MessageError) -> Self {
        RuntimeError::MessageError(err)
    }
}

impl From<PlistParseError> for RuntimeError {
    fn from(err: PlistParseError) -> Self {
        RuntimeError::MessageError(MessageError::from(err))
    }
}

impl From<BackupError> for RuntimeError {
    fn from(err: BackupError) -> Self {
        RuntimeError::BackupError(err)
    }
}

impl From<IoError> for RuntimeError {
    fn from(err: IoError) -> Self {
        RuntimeError::DiskError(err)
    }
}

impl From<rusqlite::Error> for RuntimeError {
    fn from(err: rusqlite::Error) -> Self {
        RuntimeError::DatabaseError(TableError::from(err))
    }
}

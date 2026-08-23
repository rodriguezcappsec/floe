use std::{io, path::PathBuf};

use thiserror::Error;

/// A structured failure produced while reading a local directory.
#[derive(Debug, Error)]
pub enum DirectoryError {
    #[error("could not open directory {path:?}: {source}")]
    Open { path: PathBuf, source: io::Error },

    #[error("could not read an entry in {path:?}: {source}")]
    ReadEntry { path: PathBuf, source: io::Error },

    #[error("could not read metadata for {path:?}: {source}")]
    Metadata { path: PathBuf, source: io::Error },

    #[error("directory loading was superseded by a newer request")]
    Cancelled,
}

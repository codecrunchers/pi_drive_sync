use std::fmt;
use std::path::StripPrefixError;

#[derive(Debug)]
pub enum SyncerErrors {
    InvalidPathError,
    SyncerNoneError,
}

impl From<StripPrefixError> for SyncerErrors {
    fn from(_: StripPrefixError) -> Self {
        SyncerErrors::InvalidPathError
    }
}

impl std::error::Error for SyncerErrors {}

impl fmt::Display for SyncerErrors {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            SyncerErrors::InvalidPathError => write!(f, "cannot process this path"),
            SyncerErrors::SyncerNoneError => write!(f, "sumtin ain't right"),
        }
    }
}

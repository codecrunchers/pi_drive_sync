use std::fmt;
use std::path::StripPrefixError;

#[derive(Debug)]
pub enum SyncerErrors {
    InvalidPathError,
    SyncerNoneError,
    NoAppSecret,
}
impl std::error::Error for SyncerErrors {}
pub type PiSyncResult = std::result::Result<String, SyncerErrors>;

impl From<StripPrefixError> for SyncerErrors {
    fn from(_: StripPrefixError) -> Self {
        SyncerErrors::InvalidPathError
    }
}

impl fmt::Display for SyncerErrors {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            SyncerErrors::InvalidPathError => write!(f, "cannot process this path"),
            SyncerErrors::SyncerNoneError => write!(f, "sumtin ain't right"),
            SyncerErrors::NoAppSecret => write!(f, "sumtin ain't right"),
        }
    }
}

use std::fmt;
use std::path::StripPrefixError;

#[derive(Debug)]
pub enum SyncerErrors {
    InvalidPathError,
    SyncerNoneError,
    NoAppSecret,
    ProviderError,
}
impl std::error::Error for SyncerErrors {}
pub type PiSyncResult<T> = std::result::Result<T, SyncerErrors>;

impl From<StripPrefixError> for SyncerErrors {
    fn from(_: StripPrefixError) -> Self {
        SyncerErrors::InvalidPathError
    }
}

impl fmt::Display for SyncerErrors {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            SyncerErrors::InvalidPathError => write!(f, "Cannot process this path"),
            SyncerErrors::SyncerNoneError => write!(f, "Missing a value/response/input somwehere"),
            SyncerErrors::NoAppSecret => write!(f, "Missing Auth Secret/Creds"),
            SyncerErrors::ProviderError => write!(f, "Issue with call to Storgare Provider"),
        }
    }
}

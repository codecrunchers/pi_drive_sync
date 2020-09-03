use crate::pi_err::{PiSyncResult, SyncerErrors};
use crate::upload_handler::{FileOperations, SyncableFile};
use drive3::{Comment, DriveHub, Error, File, Result};
use notify::{watcher, RecursiveMode, Watcher};
use oauth2::{
    parse_application_secret, read_application_secret, ApplicationSecret, Authenticator,
    DefaultAuthenticatorDelegate, DiskTokenStorage, MemoryStorage,
};
use std::collections::HashMap;
use std::default::Default;
use std::sync::mpsc::channel;
use std::time::Duration;
use tempfile::tempfile;

use crate::common::LOG as log;

const PI_DRIVE_SYNC_PROPS_KEY: &str = "pi_sync_id";

type Hub = drive3::DriveHub<
    hyper::Client,
    oauth2::Authenticator<
        oauth2::DefaultAuthenticatorDelegate,
        oauth2::DiskTokenStorage,
        hyper::Client,
    >,
>;

pub struct Drive3Client {
    pub hub: std::result::Result<Hub, SyncerErrors>, //TODO:?? pub hmmm
}
pub trait CloudClient {
    fn upload_file(&self, local_fs_path: &std::path::Path, parent_id: &str) -> PiSyncResult;
    fn create_dir(&self, s: SyncableFile, parent_id: &str) -> PiSyncResult;
    fn id(&self, s: SyncableFile) -> PiSyncResult;
}

impl Drive3Client {
    pub fn new(secret_file: String) -> Self {
        match Drive3Client::read_client_secret(secret_file) {
            Some(secret) => {
                let token_storage = DiskTokenStorage::new(&String::from("temp_token"))
                    .expect("Cannot create temp storage token - write permissions?");
                let auth = Authenticator::new(
                    &secret,
                    DefaultAuthenticatorDelegate,
                    hyper::Client::with_connector(hyper::net::HttpsConnector::new(
                        hyper_rustls::TlsClient::new(),
                    )),
                    token_storage,
                    Some(yup_oauth2::FlowType::InstalledInteractive),
                );

                let hub = DriveHub::new(
                    hyper::Client::with_connector(hyper::net::HttpsConnector::new(
                        hyper_rustls::TlsClient::new(),
                    )),
                    auth,
                );

                Drive3Client { hub: Ok(hub) }
            }
            None => Drive3Client {
                hub: Err(SyncerErrors::SyncerNoneError),
            },
        }
    }

    fn read_client_secret(file: String) -> Option<ApplicationSecret> {
        read_application_secret(std::path::Path::new(&file)).ok()
    }
}

impl CloudClient for Drive3Client {
    fn upload_file(&self, local_fs_path: &std::path::Path, parent_id: &str) -> PiSyncResult {
        todo!()
    }

    fn create_dir(&self, s: SyncableFile, parent_id: &str) -> PiSyncResult {
        todo!()
    }

    fn id(&self, s: SyncableFile) -> PiSyncResult {
        let local_path = s.local_path().to_str();
        trace!(log, "File Search for {:?}", local_path.clone().unwrap());

        let b64_id = s.get_unique_id()?;

        trace!(
            log,
            "B64 id  = {} for path {}",
            b64_id,
            local_path.clone().unwrap()
        );

        let q = &format!(
            "{} {{ key='{}' and value='{}' }}",
            "appProperties has ", PI_DRIVE_SYNC_PROPS_KEY, b64_id
        );

        trace!(log, "Query {:?}", q);

        let h = &self
            .hub
            .as_ref()
            .map_err(|e| SyncerErrors::NoAppSecret)
            .unwrap();

        let result = h.files().list().q(q).doit();

        match result {
            Err(e) => match e {
                Error::HttpError(_)
                | Error::MissingAPIKey
                | Error::MissingToken(_)
                | Error::Cancelled
                | Error::UploadSizeLimitExceeded(_, _)
                | Error::Failure(_)
                | Error::BadRequest(_)
                | Error::FieldClash(_)
                | Error::JsonDecodeError(_, _) => {
                    error!(log, "Failed to invoke upload api {}", e);
                    Err(SyncerErrors::SyncerNoneError)
                }
            },
            Ok(res) => {
                trace!(log, "Query Success {:?}", res);
                Ok(res
                    .1
                    .files
                    .ok_or(SyncerErrors::SyncerNoneError)?
                    .get(0)
                    .ok_or(SyncerErrors::SyncerNoneError)?
                    .id
                    .clone()
                    .unwrap())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::common::LOG as log;
    use crate::drive_cli::*;
    use crate::upload_handler::{FileOperations, SyncableFile};
    use std::io::prelude::*;
    use std::path::{Path, PathBuf, StripPrefixError};

    #[test]
    fn test_drive_cli_create_dir() {
        assert_eq!(1, 2);
    }

    #[test]
    fn test_drive_cli_upload_file() {
        assert_eq!(1, 2);
    }

    #[test]
    fn test_drive_cli_id() {
        let mut file = std::fs::File::create("/tmp/alan.txt").unwrap();
        file.write_all(b"empty_file\n").unwrap();
        let s: SyncableFile = SyncableFile::new("/tmp/pi_sync/images/alan.txt".to_string());
        let drive =
            Drive3Client::new("/home/alan/.google-service-cli/drive3-secret.json".to_owned());
        assert_eq!(drive.id(s).is_err(), true);
    }

    #[test]
    fn test_strip_local_fs() {
        assert_eq!(1, 2);
    }
}

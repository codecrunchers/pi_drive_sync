use crate::common::LOG as log;
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

const PI_DRIVE_SYNC_PROPS_KEY: &str = "pi_sync_id";
pub type Hub = drive3::DriveHub<
    hyper::Client,
    oauth2::Authenticator<
        oauth2::DefaultAuthenticatorDelegate,
        oauth2::DiskTokenStorage,
        hyper::Client,
    >,
>;

pub struct Drive3Client {
    hub: std::result::Result<Hub, SyncerErrors>, //TODO:?? pub hmmm
}

pub trait CloudClient {
    fn upload_file(
        &self,
        local_fs_path: &std::path::Path,
        parent_id: &str,
    ) -> PiSyncResult<Option<String>>;
    fn create_dir(&self, s: &SyncableFile, parent_id: Option<&str>)
        -> PiSyncResult<Option<String>>;
    fn id(&self, s: &SyncableFile) -> PiSyncResult<Option<String>>;
    fn app_props_map(&self, id: &str) -> Option<HashMap<String, String>>;
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

    pub fn get_hub(&self) -> &Hub {
        &self
            .hub
            .as_ref()
            .map_err(|e| SyncerErrors::SyncerNoneError) //?
            .unwrap()
    }

    fn read_client_secret(file: String) -> Option<ApplicationSecret> {
        read_application_secret(std::path::Path::new(&file)).ok()
    }
}

impl CloudClient for Drive3Client {
    fn app_props_map(&self, id: &str) -> Option<HashMap<String, String>> {
        let mut app_props = HashMap::new();
        app_props.insert(PI_DRIVE_SYNC_PROPS_KEY.into(), id.to_owned());
        Some(app_props)
    }

    ///Create a remote file, assigned a parent folder - and then return the Storage Service File Id
    fn upload_file(
        &self,
        local_fs_path: &std::path::Path,
        parent_id: &str,
    ) -> PiSyncResult<Option<String>> {
        todo!()
    }

    ///Create a remote dir and then return the Storage Service File Id
    fn create_dir(
        &self,
        s: &SyncableFile,
        parent_id: Option<&str>,
    ) -> PiSyncResult<Option<String>> {
        trace!(
            log,
            "Mkdir:: Dir to create {:?} from local {:?}",
            s.cloud_path(),
            s.local_path()
        );

        let temp_file = tempfile().or_else(|e| Err(SyncerErrors::InvalidPathError));
        let mut req = drive3::File::default();

        req.name = s
            .local_path()
            .file_name()
            .and_then(|p| Some(p.to_str().unwrap().to_owned()));

        req.parents = Some(vec![parent_id.unwrap_or("").to_owned()]);
        req.app_properties = self.app_props_map(&s.get_unique_id()?);
        req.mime_type = Some("application/vnd.google-apps.folder".to_string());

        let result = self.get_hub().files().create(req).upload(
            temp_file.unwrap(),
            "application/vnd.google-apps.folder".parse().unwrap(),
        );

        match result {
            Err(e) => match e {
                // The Error enum provides details about what exactly happened.
                // You can also just use its `trace`, `Display` or `Error` traits
                Error::HttpError(_)
                | Error::MissingAPIKey
                | Error::MissingToken(_)
                | Error::Cancelled
                | Error::UploadSizeLimitExceeded(_, _)
                | Error::Failure(_)
                | Error::BadRequest(_)
                | Error::FieldClash(_)
                | Error::JsonDecodeError(_, _) => {
                    error!(log, "Failed to invoke mkdir API {}", e.to_string());
                    Err(SyncerErrors::ProviderError)
                }
            },
            Ok(res) => {
                trace!(log, "Success, dir  created: {:?}", res);
                Ok(Some("".to_string())) //res.1.id.as_ref())
            }
        }
    }

    fn id(&self, s: &SyncableFile) -> PiSyncResult<Option<String>> {
        let local_path = s.local_path().to_str();
        trace!(log, "File Search for {:?}", s.cloud_path().unwrap());
        let b64_id = s.get_unique_id()?;
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
                    Err(SyncerErrors::ProviderError)
                }
            },
            Ok(res) => {
                trace!(log, "Query Success {:?}", res);
                Ok(res.1.files.and_then(|mut fv| fv.pop()?.id))
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

        let s: SyncableFile = SyncableFile::new(
            "/tmp/pi_sync/images/alan.txt".to_string(),
            Drive3Client::new("/home/alan/.google-service-cli/drive3-secret.json".to_owned()),
        );

        let r = s.storage_cli.id(&s);
        assert_eq!(r.is_ok(), true);
        assert_eq!(r.unwrap(), Some("123".to_string()));
    }

    #[test]
    fn test_strip_local_fs() {
        assert_eq!(1, 2);
    }
}

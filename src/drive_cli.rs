use crate::common::LOG as log;
use crate::pi_err::{PiSyncResult, SyncerErrors};
use crate::upload_handler::{FileOperations, SyncableFile};
use drive3::{Comment, DriveHub, Error, File, Result};
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
        local_fs_path: &str,
        parent_id: Option<&str>,
    ) -> PiSyncResult<Option<String>>;
    fn create_dir(
        &self,
        local_fs_path: &str,
        parent_id: Option<&str>,
    ) -> PiSyncResult<Option<String>>;
    fn id(&self, local_path: &str) -> PiSyncResult<Option<String>>; //should this be cloud path
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
                hub: Err(SyncerErrors::NoAppSecret),
            },
        }
    }

    pub fn get_hub(&self) -> &Hub {
        &self
            .hub
            .as_ref()
            .map_err(|e| SyncerErrors::SyncerNoneError) //TODO
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
        local_fs_path: &str,
        parent_id: Option<&str>,
    ) -> PiSyncResult<Option<String>> {
        let s = SyncableFile::new(local_fs_path.to_owned());
        trace!(
            log,
            "Create dir:: Dir to create {:?} from local {:?}",
            s.cloud_path(),
            s.local_path()
        );

        let mut req = drive3::File::default();
        req.name = Some(
            s.local_path()
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .to_owned(),
        );
        req.parents = parent_id.and_then(|p| Some(vec![p.to_owned()]));
        req.app_properties = self.app_props_map(&s.get_unique_id()?);
        trace!(log, "Upload Req {:?}", req);

        // Values shown here are possibly random and not representative !
        let result = self
            .get_hub()
            .files()
            .create(req)
            .use_content_as_indexable_text(true)
            .supports_team_drives(false)
            .supports_all_drives(true)
            .keep_revision_forever(false)
            .ignore_default_visibility(true)
            .enforce_single_parent(true)
            .upload_resumable(
                std::fs::File::open(local_fs_path).unwrap(),
                "application/octet-stream".parse().unwrap(),
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
                    error!(log, "Failed to invoke upload api {}", e);
                    Err(SyncerErrors::ProviderError)
                }
            },
            Ok(res) => {
                trace!(log, "Upload Call Success: {:?}", res);
                Ok(res.1.id.clone())
            }
        }
    }

    ///Create a remote dir in root offset relatie to local_dir and then return the Storage Service File Id
    fn create_dir(
        &self,
        local_fs_path: &str,
        parent_id: Option<&str>,
    ) -> PiSyncResult<Option<String>> {
        let s = SyncableFile::new(local_fs_path.to_owned());
        trace!(
            log,
            "Create dir:: Dir to create {:?} from local {:?}",
            s.cloud_path(),
            s.local_path()
        );

        let temp_file = tempfile().or_else(|e| {
            error!(log, "Cannot create temp file");
            Err(SyncerErrors::InvalidPathError)
        });

        let mut req = drive3::File::default();
        req.name = s
            .local_path()
            .file_name()
            .and_then(|p| Some(p.to_str().unwrap().to_owned()));

        req.parents = parent_id.and_then(|p| Some(vec![p.to_owned()]));
        req.app_properties = self.app_props_map(&s.get_unique_id()?);
        req.mime_type = Some("application/vnd.google-apps.folder".to_string());

        trace!(log, "Sending Request {:?}", req);

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
                    error!(log, "Failed to invoke mkdir API {:?}", e);
                    Err(SyncerErrors::ProviderError)
                }
            },
            Ok(res) => {
                trace!(log, "Success, dir  created: {:?}", res);
                Ok(res.1.id.clone())
            }
        }
    }

    ///Get the google drive id for this entry
    fn id(&self, local_path: &str) -> PiSyncResult<Option<String>> {
        trace!(log, "Search for Google Drive Id for {}", local_path);
        let s = SyncableFile::new(local_path.to_owned());
        let b64_id = s.get_unique_id()?;
        debug!(
            log,
            "Base64 Unique ID = {} for file {:?}", b64_id, local_path
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
                    Err(SyncerErrors::ProviderError)
                }
            },
            Ok(res) => {
                trace!(log, "Query Success {:?}", res);
                Ok(res.1.files.and_then(|mut fv| {
                    if fv.len() > 1 {
                        warn!(
                            log,
                            "More than one file returned when searching for pi_sync_id = {:?}",
                            b64_id
                        );
                    }
                    fv.pop()?.id
                }))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::common::LOG as log;
    use crate::drive_cli::*;
    use crate::upload_handler::{
        FileOperations, SyncableFile, DRIVE_ROOT_FOLDER, LOCAL_ROOT_FOLDER,
    };
    use std::io::prelude::*;
    use std::path::{Path, PathBuf, StripPrefixError};

    #[test]
    fn test_drive_cli_create_dir() {
        let dc = Drive3Client::new("/home/alan/.google-service-cli/drive3-secret.json".to_owned());
        let d = "/tmp/pi_sync/images/new_dir";
        let r = dc.create_dir(d, None);
        println!("Id of new Dir {:?}", r);
        assert_eq!(r.is_ok(), true);
    }

    #[test]
    fn test_drive_cli_upload_file() {
        todo!()
    }

    ///TODO: this is leaveing state on provider, will fail on second run
    #[test]
    fn test_drive_cli_id() {
        let dc = Drive3Client::new("/home/alan/.google-service-cli/drive3-secret.json".to_owned());
        let d = "/tmp/pi_sync/images/new_dir";

        /*let parent_on_fs_id = SyncableFile::new(d.to_owned())
            .parent_path()
            .and_then(|pp| SyncableFile::new(pp.to_str().unwrap().to_owned()).get_unique_id())
            .unwrap();
        */

        let drive_id_for_parent = dc.id(SyncableFile::new(d.to_owned())
            .parent_path()
            .unwrap()
            .to_str()
            .unwrap());

        println!(" parent gdrive id = {:?}", drive_id_for_parent);
        assert_eq!(1, 2);

        /*
        let id_returned_from_call = dc.create_dir(d, "drive_id_for_parent.ok().unwrap()).unwrap();
        let id_returned_from_cloud_provider_lookup = dc.id(d);


        assert_eq!(
            id_returned_from_cloud_provider_lookup.ok().unwrap(),
            id_returned_from_call
        );

        */
        /*        let child_d = "/tmp/pi_sync/images/new_dir/child_dir";
                let child_id_response = dc.create_dir(child_d, Some(&api_id.unwrap())).unwrap();
                let child_remote_id = dc.id(d);
                assert_eq!(child_remote_id.is_ok(), true);
                assert_eq!(child_remote_id.unwrap(), child_id_response);
        */
        //let mut file = std::fs::File::create("/tmp/pi_sync/images/alan.txt").unwrap();
        //file.write_all(b"empty_file\n").unwrap();

        
    }
}

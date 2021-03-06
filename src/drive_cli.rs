use crate::common::LOG as log;
use crate::pi_err::{PiSyncResult, SyncerErrors};
use crate::upload_handler::{FileOperations, SyncableFile};
use drive3::{DriveHub, Error};
use yup_oauth2::{
    read_application_secret, ApplicationSecret, Authenticator, DefaultAuthenticatorDelegate,
    DiskTokenStorage, GetToken, MemoryStorage,
};
//use crate::oauth2::GetToken;`

use regex::Regex;
use std::collections::HashMap;
use std::default::Default;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tempfile::tempfile;
use ttl_cache::TtlCache;

lazy_static::lazy_static! {
    static ref CACHE_TTL: std::time::Duration = Duration::new(86400, 0);
}

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
    hub: std::result::Result<Hub, SyncerErrors>,
    filters: Vec<String>,
    cache: Arc<RwLock<TtlCache<String, String>>>,
}

pub trait CloudClient {
    fn upload_file(&self, local_fs_path: &str) -> PiSyncResult<Option<String>>;
    fn create_dir(
        &self,
        local_fs_path: &str,
        parent_id: Option<&str>,
    ) -> PiSyncResult<Option<String>>;
    fn id(&self, local_path: &str) -> PiSyncResult<Option<String>>; //should this be cloud path
    fn app_props_map(&self, id: &str) -> Option<HashMap<String, String>>;
    fn passes_filter(&self, local_fs_path: &str) -> bool;
}

impl Drive3Client {
    pub fn new(secret_file: String, filters: Vec<&str>) -> Self {
        let cache = Arc::new(RwLock::new(TtlCache::new(100)));

        match Drive3Client::read_client_secret(secret_file) {
            Some(secret) => {
                let token_storage = DiskTokenStorage::new(&String::from("temp_token"))
                    .expect("Cannot create temp storage token - write permissions?");

                let mut auth = Authenticator::new(
                    &secret,
                    DefaultAuthenticatorDelegate,
                    hyper::Client::with_connector(hyper::net::HttpsConnector::new(
                        hyper_rustls::TlsClient::new(),
                    )),
                    token_storage,
                    Some(yup_oauth2::FlowType::InstalledInteractive),
                );

                let scopes = &[
                    "https://www.googleapis.com/auth/drive",
                    "https://www.googleapis.com/auth/drive.metadata.readonly",
                ];

                match auth.token(scopes) {
                    Err(e) => println!("error: {:?}", e),
                    Ok(t) => println!("The token is {:?}", t),
                };

                let hub = DriveHub::new(
                    hyper::Client::with_connector(hyper::net::HttpsConnector::new(
                        hyper_rustls::TlsClient::new(),
                    )),
                    auth,
                );

                Drive3Client {
                    hub: Ok(hub),
                    filters: filters
                        .iter()
                        .map(|x| x.to_string())
                        .collect::<Vec<String>>(),
                    cache: cache,
                }
            }
            None => Drive3Client {
                hub: Err(SyncerErrors::NoAppSecret),
                filters: vec![],
                cache: cache,
            },
        }
    }

    pub fn get_hub(&self) -> PiSyncResult<&Hub> {
        self.hub.as_ref().map_err(|_e| SyncerErrors::NoAppSecret)
    }

    fn read_client_secret(file: String) -> Option<ApplicationSecret> {
        read_application_secret(std::path::Path::new(&file)).ok()
    }

    ///TODO: Panic Central
    fn create_path(&self, syncable: &SyncableFile) -> PiSyncResult<bool> {
        debug!(log, "create path for {:?}", syncable.local_path());

        let mut rel_path = syncable
            .local_path()
            .strip_prefix("/var/www/RpiCamera")
            .unwrap();

        let components: Vec<_> = rel_path.components().map(|comp| comp.as_os_str()).collect();

        debug!(log, "components {:?}", components);

        let file_name_index = components.len() - 1;
        let mut last_dir = String::from("/var/www/RpiCamera/");
        for (path_index, dir) in components.iter().enumerate() {
            if path_index != file_name_index {
                let d = dir.to_str().unwrap();
                let dir_to_create = format!("{}{}", last_dir, d);
                let b64_id = SyncableFile::new(dir_to_create.clone()).get_unique_id()?;
                debug!(log, "Cache check for {:?}", dir_to_create.clone());
                let c_lock = Arc::clone(&self.cache);
                //if not in cache
                if !c_lock.read().unwrap().contains_key(
                    &SyncableFile::new(dir_to_create.clone())
                        .get_unique_id()
                        .unwrap(),
                ) {
                    //if does exist on disk
                    let drive_id = self.id(&dir_to_create).ok().and_then(|id| id);
                    if drive_id.is_some() {
                        trace!(
                            log,
                            "create_path: Adding Cache Entry for Existing dir {} with drive_id {:?}",
                            dir_to_create,
                            drive_id
                        );
                        // let c_lock = Arc::clone(&self.cache);
                        let mut c_lock = c_lock.write().unwrap();
                        c_lock.insert(b64_id.clone(), drive_id.unwrap(), *CACHE_TTL);
                    } else {
                        //create it now, then cache it
                        let parent_id = self //check_cache
                            .id(&last_dir)
                            .ok()
                            .and_then(|o| o)
                            .and_then(|s| Some(s))
                            .unwrap();

                        match self
                            .create_dir(&dir_to_create.to_owned(), Some(&parent_id.to_owned()))
                        {
                            Ok(did) => match did {
                                Some(drive_id) => {
                                    let c_lock = Arc::clone(&self.cache);
                                    let mut cached_w = c_lock.write().unwrap();
                                    let x = cached_w.insert(
                                        b64_id.clone(),
                                        drive_id.clone(),
                                        *CACHE_TTL,
                                    );
                                    debug!(
                            log,
                            "create_path: Cache Entry Added for new dir = {} , uid={}, drive_id={:?}",
                            dir_to_create,
                            b64_id,
                            drive_id.to_owned()
                        );
                                }
                                _ => warn!(
                                    log,
                                    "invalid drive id returned from call to create dir: {:?}",
                                    drive_id
                                ),
                            },
                            Err(e) => error!(
                                log,
                                "invalid drive id returned from call to create dir: {:?}", drive_id
                            ),
                        }
                    }
                } else {
                    debug!(log, "Cache hit for {:?}, not creating dir", dir_to_create);
                }
            }
            //build up the parent path hierarchy with root and last created dir concats
            last_dir.push_str(format!("{}/", dir.to_str().unwrap().to_string()).as_str());
        }
        Ok(true) //TODO: can panic
    }
}

impl CloudClient for Drive3Client {
    fn app_props_map(&self, id: &str) -> Option<HashMap<String, String>> {
        let mut app_props = HashMap::new();
        app_props.insert(PI_DRIVE_SYNC_PROPS_KEY.into(), id.to_owned());
        Some(app_props)
    }

    fn passes_filter(&self, local_fs_path: &str) -> bool {
        let s = SyncableFile::new(local_fs_path.to_owned());
        let filename = s.get_filename().unwrap(); //TODO: p!
        trace!(log, "Check Filter for {}", filename);

        if self.filters.len() == 0 {
            trace!(log, "No filters enabled, {:?} allowed", &s.get_filename());
            true
        } else {
            let matched = self
                .filters
                .iter()
                .map(|val| {
                    Regex::new(&val).ok().and_then(|re: Regex| {
                        trace!(log, "Checking {:} against {:?} ", &filename, re);
                        re.captures(&filename)
                    })
                })
                .map(|captures| captures.is_some())
                .fold(false, |ordd, regexp_find| {
                    trace!(log, "{:} || {:?} ", ordd, regexp_find);
                    ordd || regexp_find
                });
            debug!(log, "Passes Filter = {}", matched);
            matched
        }
    }

    ///Create a remote file, assigned a parent folder - and then return the Storage Service File Id
    fn upload_file(&self, local_fs_path: &str) -> PiSyncResult<Option<String>> {
        let s = SyncableFile::new(local_fs_path.to_owned());

        trace!(
            log,
            "Upload File:: Cloud Target={:?} from local {:?}",
            s.cloud_path(),
            s.local_path(),
        );

        //build the ancestor file tree on provider if we don't have it
        self.create_path(&s);

        let parent_path = s.parent_path().unwrap();
        let parent_path = parent_path.to_str().unwrap();

        let mut req = drive3::File::default();
        req.name = Some(
            s.local_path()
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .to_owned(),
        );

        let parent_path = s.parent_path().unwrap(); //TODO p!
        let parent_path = parent_path.to_str().unwrap(); //TODO: p!

        let parent_id = self.id(parent_path).ok(); // TODO: p!
        trace!(log, "Parent Id for {:?}=  {:?}", parent_path, parent_id);

        req.parents = parent_id.unwrap().and_then(|p| Some(vec![p.to_owned()])); //TODO P!
        req.app_properties = self.app_props_map(&s.get_unique_id()?);
        trace!(log, "Upload Req {:?}", req);

        // Values shown here are possibly random and not representative !
        //handle file deletion
        if let Ok(file) = std::fs::File::open(local_fs_path) {
            let result = self
                .get_hub()?
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
                    let drive_id = res.1.id.clone();
                    let drive_id = drive_id.unwrap();
                    Ok(Some(drive_id.into()))
                }
            }
        } else {
            error!(log, "File deleted before we got to it");
            Err(SyncerErrors::ProviderError)
        }
    }

    ///Create a remote dir in root offset relative, from target-dir
    ///and then return the Storage Service File Id
    fn create_dir(
        &self,
        local_fs_path: &str,
        parent_id: Option<&str>,
    ) -> PiSyncResult<Option<String>> {
        let s = SyncableFile::new(local_fs_path.to_owned());
        trace!(
            log,
            "Create dir:: Remote Dir to create {:?} from local {:?}",
            s.cloud_path(),
            s.local_path()
        );

        let temp_file = tempfile().or_else(|_e| {
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

        let result = self.get_hub()?.files().create(req).upload(
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
                let uid = s.get_unique_id().unwrap();

                let drive_id = res.1.id.clone();
                let drive_id = drive_id.unwrap();
                trace!(
                    log,
                    "localpath = {} ,uid = {:?}, drive id ={}",
                    s.local_path().to_str().unwrap(),
                    uid,
                    drive_id.clone()
                );

                let c_lock = Arc::clone(&self.cache);
                let mut cached_w = c_lock.write().unwrap();
                let x = cached_w.insert(uid.clone(), drive_id.clone(), *CACHE_TTL);
                debug!(
                    log,
                    "Cache Entry Added for uid={}, dir={}, drive_id={}",
                    uid,
                    local_fs_path,
                    &drive_id
                );

                Ok(Some(drive_id.into()))
            }
        }
    }

    ///Query Google for the pi-sync-id, validating if this dir exists or not
    fn id(&self, local_path: &str) -> PiSyncResult<Option<String>> {
        trace!(log, "Search for Google Drive Id for {}", local_path);
        let s = SyncableFile::new(local_path.to_owned());
        let b64_id = s.get_unique_id()?;
        debug!(
            log,
            "Base64 Unique ID = {} for file {:?}", b64_id, local_path
        );
        //{
        /*let c_lock = Arc::clone(&self.cache);
            //check cache
            if c_lock.read().unwrap().contains_key(&b64_id) {
                trace!(log, "Get Drive ID, cache hit for {}", local_path);
                return Ok(Some(
                    c_lock.read().unwrap().get(&b64_id).unwrap().to_owned(),
                ));
            }
        }*/

        let q = &format!(
            "{} {{ key='{}' and value='{}' }} and {} = {}",
            "appProperties has ", PI_DRIVE_SYNC_PROPS_KEY, b64_id, "trashed", "false"
        );

        trace!(log, "Query {:?}", q);

        let h = &self.get_hub()?;

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
                trace!(log, "Id Query Success {:?}", res);
                Ok(res.1.files.and_then(|mut fv| {
                    let drive_id = fv.pop()?.id.unwrap();

                    if fv.len() > 1 {
                        warn!(
                            log,
                            "More than one file returned when searching for pi_sync_id = {:?}",
                            b64_id
                        );
                    }
                    Some(drive_id)
                }))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::drive_cli::*;
    use crate::upload_handler::{FileOperations, SyncableFile};

    #[test]
    fn test_drive_cli_create_dir() {
        let dc = Drive3Client::new(
            "/home/alan/.google-service-cli/drive3-secret.json".to_owned(),
            vec![],
        );
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
        let dc = Drive3Client::new(
            "/home/alan/.google-service-cli/drive3-secret.json".to_owned(),
            vec![],
        );
        let d = "/tmp/pi_sync/images/new_dir";

        let drive_id_for_parent = dc.id(SyncableFile::new(d.to_owned())
            .parent_path()
            .unwrap()
            .to_str()
            .unwrap());

        println!(" parent gdrive id = {:?}", drive_id_for_parent);
        assert_eq!(1, 2);
    }

    #[test]
    fn test_create_path() {
        let s = Drive3Client::create_path(&SyncableFile::new(
            "/var/www/RpiCamera/1/2/3/4/im1.jpg".to_string(),
        ));
        assert_eq!(true, s.is_ok());
    }
}

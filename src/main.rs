#[macro_use]
extern crate slog;
#[macro_use]
extern crate lazy_static;
extern crate base64;
extern crate google_drive3 as drive3;
extern crate hyper;
extern crate hyper_rustls;
extern crate notify;
extern crate tempfile;
extern crate yup_oauth2 as oauth2;

mod upload_handler;
use self::base64::{decode, encode};
use clap::{App, Arg, SubCommand};
use drive3::{Comment, DriveHub, Error, File, Result};
use notify::{watcher, RecursiveMode, Watcher};
use oauth2::{
    parse_application_secret, read_application_secret, ApplicationSecret, Authenticator,
    DefaultAuthenticatorDelegate, DiskTokenStorage, MemoryStorage,
};
use std::collections::HashMap;
use std::default::Default;
use std::path::Path;
use std::sync::mpsc::channel;
use std::time::Duration;
use tempfile::tempfile;
use upload_handler::*;
mod common;
use slog::Logger;

const DIR_SCAN_DELAY: &str = "1";
const ROOT_FOLDER_ID: &str = "19ipt2Rg1TGzr5esE_vA_1oFjrt7l5g7a"; //TODO, needs to be smarter
const LOCAL_FILE_STORE: &str = "/tmp/pi_sync/images";
const ROOT_OF_MD5: &str = "RpiCamSyncer";
const IS_ROOT: usize = 1;

//save me typing this for sigs
type Hub = drive3::DriveHub<
    hyper::Client,
    oauth2::Authenticator<
        oauth2::DefaultAuthenticatorDelegate,
        oauth2::DiskTokenStorage,
        hyper::Client,
    >,
>;

use common::LOG as log;

fn read_client_secret(file: String) -> ApplicationSecret {
    read_application_secret(std::path::Path::new(&file)).expect("No App Secret")
}

fn main() {
    trace!(log, "Statring Syncer");

    let matches = App::new("Rusty Cam Syncer")
        .version("1.0")
        .author("Alan R. <alan@alanryan.name.com>")
        .version("1.0")
        .about("Will Sync a Dir recursvively with the smarts of a sheep")
        .arg(
            Arg::with_name("secret_file")
                .short("s")
                .long("secret_file")
                .value_name("secret_file")
                .help("Where to find Google Drive API JSON secrets")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("target_dir")
                .short("t")
                .long("target_dir")
                .value_name("target_dir")
                .help("Directory to monitor and sync")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("scan_interval_seconds")
                .short("i")
                .long("scan_interval_seconds")
                .value_name("scan_interval_seconds")
                .help("Directory to monitor and sync")
                .takes_value(true),
        )
        .get_matches();

    let secret_file = matches
        .value_of("secret_file")
        .unwrap_or("/home/alan/.google-service-cli/drive3-secret.json");

    let target_dir = matches.value_of("target_dir").unwrap_or(LOCAL_FILE_STORE);

    let scan_interval_seconds = String::from(
        matches
            .value_of("scan_interval_seconds")
            .unwrap_or(DIR_SCAN_DELAY),
    )
    .parse::<u64>()
    .unwrap();

    trace!(log, "Using {} as WPS Script", secret_file);
    trace!(log, "Using {} as Dir to monitor", target_dir);

    let secret = read_client_secret(secret_file.to_string());

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

    let mut hub = DriveHub::new(
        hyper::Client::with_connector(hyper::net::HttpsConnector::new(
            hyper_rustls::TlsClient::new(),
        )),
        auth,
    );

    //TODO: first run
    //if ! -d "RpiCamSyncer"
    mkdir(&hub, "/RpiCamSyncer-ignore");

    let (sender, receiver) = channel();
    let mut watcher =
        watcher(sender, Duration::from_secs(scan_interval_seconds)).expect("Cannot Create Watcher");
    watcher
        .watch(target_dir, RecursiveMode::Recursive)
        .expect("Canot Watch Dir");

    let handle_event = |h, p: std::path::PathBuf| {
        if let Some(path) = p.to_str() {
            if std::path::Path::new(path).is_dir() {
                mkdir(&hub, path);
            } else {
                upload(&hub, path);
            }
        } else {
            warn!(log, "Cannot Create {:?}", p);
        }
    };

    loop {
        let hub = &hub;
        match receiver.recv() {
            Ok(notify::DebouncedEvent::Create(p)) => {
                handle_event(hub, p.clone());
            }
            _ => trace!(log, "unidentified event"),
            Err(e) => error!(log, "watch error: {:?}", e),
        }
    }
}

///Return base64 of Parent  [ROOT_ID + File Name] - will work for now, clashes possible with poor
///encoder ///expecting file to be YYYYMMDDHHSS.[?]
fn get_unique_entry_id(base_path: &str) -> Option<String> {
    trace!(log,"get_unique_entry_id for {:?}", base_path; "base_path"=>true);
    strip_local_fs(base_path).and_then(|file_name| {
        trace!(log,"get_unique_entry_id {:?}", file_name; "relative_path"=>true);
        let base64_buf = encode(format!("{}{}", ROOT_OF_MD5, &file_name));
        trace!(log,"get_unique_entry_id as b64 {:?}", base64_buf; "x"=>1);
        Some(base64_buf.clone())
    })
}

fn strip_local_fs(lfsn: &str) -> Option<&str> {
    debug!(log,"strip_local_fs"; "lfsn"=>lfsn);

    if lfsn.len() > LOCAL_FILE_STORE.len() && lfsn[0..LOCAL_FILE_STORE.len()].eq(LOCAL_FILE_STORE) {
        std::path::Path::new(&lfsn[LOCAL_FILE_STORE.len() + 1..]).to_str()
    } else {
        Some("")
    }
}

fn get_file_name(lfsn: &str) -> Option<&str> {
    trace!(log,"get_file_name"; "name"=>lfsn);
    let path = std::path::Path::new(lfsn);
    path.file_name()?.to_str()
}

fn parent_id_as_base_64(local_path: &str) -> Option<String> {
    debug!(log, "Path to id for {}", local_path);

    if strip_local_fs(local_path).unwrap().len() == IS_ROOT {
        debug!(log, "File Search Result {}",strip_local_fs(local_path).unwrap().len(); "is_root"=>true);
        None
    } else {
        debug!(log, "File Search Result"; "is_root"=>false, "len"=>strip_local_fs(local_path).unwrap().len());
        match std::path::Path::new(local_path).is_file() {
            true => {
                let mut ancestors = Path::new(local_path).ancestors().next();
                trace!(
                    log,
                    "ancestors={:?}, len = {}",
                    &ancestors,
                    strip_local_fs(local_path).unwrap().len()
                );
                get_unique_entry_id(ancestors?.file_name()?.to_str()?)
            }
            false => get_unique_entry_id(local_path),
        }
    }
}

fn local_dir_to_drive_file_id(hub: &Hub, path: &str) -> std::result::Result<u16, String> {
    //hyper::client::Response, drive3::DriveList)> {
    info!(log, "File Search for"; "path"=>path);

    let b64_id = parent_id_as_base_64(path)
        .unwrap_or(ROOT_FOLDER_ID.to_owned()) //here we use root
        .as_str()
        .to_owned();

    trace!(log, "B64 id  = {} for path {}", b64_id, path);

    let q = &format!(
        "{} {{ key='{}' and value='{}' }}",
        "appProperties has ", PI_DRIVE_SYNC_PROPS_KEY, b64_id
    );

    trace!(log, "Query {:?}", q);

    let result = hub.drives().list().q(q).doit();

    match result {
        Err(e) => match e {
            // The Error enum provides details about what exactly happened.
            // You can also just use its `Debug`, `Display` or `Error` traits
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
                Err(e.to_string())
            }
        },
        Ok(res) => {
            trace!(log, "Success Upload: {:?}", res);
            Ok(res.0.status.to_u16())
        }
    }
}

const PI_DRIVE_SYNC_PROPS_KEY: &str = "pi_sync-id";

fn app_props_map(id: &str) -> Option<HashMap<String, String>> {
    let mut app_props = HashMap::new();
    app_props.insert(PI_DRIVE_SYNC_PROPS_KEY.into(), id.to_owned());
    Some(app_props)
}

fn upload(hub: &Hub, path: &str) -> std::result::Result<u16, String> {
    trace!(log,"entering"; "method"=>"upload");
    let mut req = drive3::File::default();
    req.name = get_file_name(path).and_then(|p| Some(p.into()));
    let id = get_unique_entry_id(path).unwrap();
    trace!(log, "Added Drive Idr"; "id"=>&id);
    req.app_properties = app_props_map(&id);

    local_dir_to_drive_file_id(&hub, &id)
        .and_then(|drive_id| Ok(req.parents = { Some(vec!["".to_owned()]) }))
        .expect("drive id conversion fail");

    trace!(log, "Upload Req {:?}", req);

    // Values shown here are possibly random and not representative !
    let result = hub
        .files()
        .create(req)
        .use_content_as_indexable_text(true)
        .supports_team_drives(false)
        .supports_all_drives(true)
        .keep_revision_forever(false)
        .ignore_default_visibility(true)
        .enforce_single_parent(true)
        .upload_resumable(
            std::fs::File::open(path).unwrap(),
            "application/octet-stream".parse().unwrap(),
        );

    match result {
        Err(e) => match e {
            // The Error enum provides details about what exactly happened.
            // You can also just use its `Debug`, `Display` or `Error` traits
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
                Err(e.to_string())
            }
        },
        Ok(res) => {
            trace!(log, "Success Upload: {:?}", res);
            Ok(res.0.status.to_u16())
        }
    }
}

///expecting folders to be hierarcihal so
/// 2020 -> [01,02,...12] W 0[*]: {1..31}
fn mkdir(hub: &Hub, path: &str) -> std::result::Result<u16, String> {
    trace!(log, "Mkdir:: Dir to create: {:?}", path);
    let mut temp_file = tempfile().unwrap();
    let mut req = drive3::File::default();

    req.name = get_file_name(path).and_then(|p| Some(p.into()));

    let id = get_unique_entry_id(path).unwrap(); //.or_else(return Err("Cannot create file id");

    local_dir_to_drive_file_id(&hub, &path)
        .and_then(|drive_id| Ok(req.parents = { Some(vec!["".to_owned()]) }))
        .expect("drive id conversion fail");
    trace!(log, "Upload Req {:?}", req);

    req.app_properties = app_props_map(&id);
    trace!(log, "Added Drive Idr"; "id"=>id);

    req.mime_type = Some("application/vnd.google-apps.folder".to_string());

    trace!(log, "Mkdir Req {:?}", req);

    // Values shown here are possibly random and not representative !
    let result = hub.files().create(req).upload(
        temp_file,
        "application/vnd.google-apps.folder".parse().unwrap(),
    );

    match result {
        Err(e) => match e {
            // The Error enum provides details about what exactly happened.
            // You can also just use its `Debug`, `Display` or `Error` traits
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
                Err(e.to_string())
            }
        },
        Ok(res) => {
            trace!(log, "Success, dir  created: {:?}", res);
            Ok(res.0.status.to_u16())
        }
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn test_strip_local_fs() {
        assert_eq!(
            Some("123"),
            crate::strip_local_fs(&format!("{}/{}", crate::LOCAL_FILE_STORE, "123"))
        );

        assert_eq!(
            Some("?123"),
            crate::strip_local_fs(&format!("{}/{}", crate::LOCAL_FILE_STORE, "?123"))
        );

        assert_eq!(
            Some("\\123"),
            crate::strip_local_fs(&format!("{}/{}", crate::LOCAL_FILE_STORE, "\\123"))
        );

        assert_eq!(
            Some("tmp/a/b/c/123"),
            crate::strip_local_fs(&format!("{}/{}", crate::LOCAL_FILE_STORE, "tmp/a/b/c/123"))
        );

        assert_eq!(
            Some(""),
            crate::strip_local_fs(&format!("{}", crate::LOCAL_FILE_STORE))
        );
    }

    #[test]
    fn test_get_unique_entry_id_dir() {
        let b64 =
            crate::get_unique_entry_id(&format!("{}{}", crate::LOCAL_FILE_STORE, "/test/1/2/"));
        println!("{:?}", b64);
        assert_eq!(String::from("UnBpQ2FtU3luY2VyMg=="), b64.unwrap());
    }

    #[test]
    fn test_create_unique_dir_id_for_root_entry() {
        let b64 = crate::get_unique_entry_id(&format!("{}{}", crate::LOCAL_FILE_STORE, "/test"));
        println!("{:?}", b64);
        assert_eq!(String::from("UnBpQ2FtU3luY2VydGVzdA=="), b64.unwrap());
    }

    #[test]
    fn test_create_unique_dir_id_for_root_root_entry() {
        let b64 = crate::get_unique_entry_id("/test");
        println!("{:?}", b64);
        assert_eq!(String::from("UnBpQ2FtU3luY2VydGVzdA=="), b64.unwrap());
    }

    #[test]
    fn test_parent_id_as_base_64() {
        let pid = crate::parent_id_as_base_64(&format!("{}{}", crate::LOCAL_FILE_STORE, "/test"));
        println!("{:?}", pid);
        assert_eq!(pid, None)
    }

    #[test]
    fn test_get_unique_entry_id() {
        let b64 = crate::get_unique_entry_id(&format!(
            "{}{}",
            crate::LOCAL_FILE_STORE,
            "/test/1/2/abc.txt"
        ));
        println!("{:?}", b64);
        assert_eq!(String::from("UnBpQ2FtU3luY2VyYWJjLnR4dA=="), b64.unwrap());
    }
}

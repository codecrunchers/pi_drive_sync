#[macro_use]
extern crate slog;
#[macro_use]
extern crate lazy_static;
extern crate google_drive3 as drive3;
extern crate hyper;
extern crate hyper_rustls;
extern crate md5;
extern crate notify;
extern crate tempfile;
extern crate yup_oauth2 as oauth2;

use clap::{App, Arg, SubCommand};
use drive3::{Comment, DriveHub, Error, File, Result};
use md5::compute;
use notify::{watcher, RecursiveMode, Watcher};
use oauth2::{
    parse_application_secret, read_application_secret, ApplicationSecret, Authenticator,
    DefaultAuthenticatorDelegate, DiskTokenStorage, MemoryStorage,
};
use std::default::Default;
use std::path::Path;
use std::sync::mpsc::channel;
use std::time::Duration;
use tempfile::tempfile;
mod common;
use slog::{Fuse, Logger};

const DIR_SCAN_DELAY: u64 = 1;
const ROOT_FOLDER_ID: &str = "19ipt2Rg1TGzr5esE_vA_1oFjrt7l5g7a"; //TODO, needs to be smarter
const LOCAL_FILE_STORE: &str = "/tmp/pi_sync/images";
const ROOT_OF_MD5: &str = "RpiCamSyncer";

//save me typing this for sigs
type Hub = drive3::DriveHub<
    hyper::Client,
    oauth2::Authenticator<
        oauth2::DefaultAuthenticatorDelegate,
        oauth2::DiskTokenStorage,
        hyper::Client,
    >,
>;

lazy_static! {
    pub static ref log: slog::Logger = Logger::root(
        Fuse(common::PrintlnDrain),
        o!("version" => "1", "app" => " Rusty Cam Syncer"),
    );
}

fn read_client_secret(file: String) -> ApplicationSecret {
    read_application_secret(std::path::Path::new(&file)).expect("No App Secret")
}

fn main() {
    info!(log, "Statring Syncer");

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

    info!(log, "Using {} as WPS Script", secret_file);
    info!(log, "Using {} as Dir to monitor", target_dir);

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
    //mkdir(&hub, "RpiCamSyncer");

    let (sender, receiver) = channel();
    let mut watcher =
        watcher(sender, Duration::from_secs(DIR_SCAN_DELAY)).expect("Cannot Watch Dir");
    watcher.watch(target_dir, RecursiveMode::Recursive).unwrap();

    loop {
        match receiver.recv() {
            Ok(notify::DebouncedEvent::Create(p)) => {
                info!(log, "Create Request{:?}", p);
                if let Some(path) = p.to_str() {
                    if std::path::Path::new(path).is_dir() {
                        mkdir(&hub, path);
                    } else {
                        upload(&hub, path);
                    }
                } else {
                    warn!(log, "Cannot Create {:?}", p);
                }
            }
            _ => info!(log, "unidentified event"),
            Err(e) => error!(log, "watch error: {:?}", e),
        }
    }
}

///Return  MD5 Hash of Parent  [ROOT_ID + File Name]
///expecting file to be YYYYMMDDHHSS.[?]
fn create_unique_file_id(local_path: &str) -> Option<String> {
    get_file_name(local_path).and_then(|file_name| {
        let md5_buf = md5::compute(format!("{}_{}", ROOT_OF_MD5, &file_name));
        Some(String::from_utf8_lossy(&md5_buf.clone().to_vec()).into_owned())
    })
}

///Return  MD5 Hash of Parent  [ROOT_ID + File Name]
///expecting file to be YYYYMMDDHHSS.[?]
fn create_unique_dir_id(local_dir: &str) -> Option<String> {
    let path = std::path::Path::new(strip_local_fs(local_dir)); //TODO p! on /
    let dir_name = path.file_name()?.to_str()?.to_string();
    let md5_buf_w_base = md5::compute(format!("{}_{}", ROOT_OF_MD5, &dir_name).as_bytes());
    info!(log,"create_unique_dir_id"; "name"=>dir_name);
    Some(String::from_utf8_lossy(&md5_buf_w_base.clone().to_vec()).into_owned())
}

fn strip_local_fs(lfsn: &str) -> &str {
    &lfsn[LOCAL_FILE_STORE.len()..]
}

fn get_file_name(lfsn: &str) -> Option<&str> {
    let path = std::path::Path::new(strip_local_fs(lfsn)); //TODO p! on /
    path.file_name()?.to_str()
}

fn upload(hub: &Hub, path: &str) -> std::result::Result<u16, String> {
    info!(log,"entering"; "method"=>"upload");
    //relative to base of Drive
    let mut req = drive3::File::default();
    req.name = Some("new_file".to_string());
    req.id = create_unique_file_id(path);
    req.parents = Some(vec![ROOT_FOLDER_ID.to_owned()]);
    info!(log, "Upload Req {:?}", req);

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
            info!(log, "Success Upload: {:?}", res);
            Ok(res.0.status.to_u16())
        }
    }
}

///expecting folders to be hierarcihal so
/// 2020 -> [01,02,...12] W 0[*]: {1..31}
fn mkdir(hub: &Hub, path: &str) -> std::result::Result<u16, String> {
    info!(log, "Mkdir:: Dir to create: {:?}", path);
    let mut temp_file = tempfile().unwrap();
    let mut req = drive3::File::default();

    req.name = Some("new_dir".to_owned());
    req.id = create_unique_dir_id(path);
    req.parents = Some(vec![ROOT_FOLDER_ID.to_owned()]);
    req.mime_type = Some("application/vnd.google-apps.folder".to_string());

    info!(log, "Mkdir Req {:?}", req);

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
            info!(log, "Success, dir  created: {:?}", res);
            Ok(res.0.status.to_u16())
        }
    }
}

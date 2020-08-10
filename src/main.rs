#[macro_use]
extern crate slog;
#[macro_use]
extern crate lazy_static;

extern crate google_drive3 as drive3;
extern crate hyper;
extern crate hyper_rustls;
extern crate notify;
extern crate tempfile;
extern crate yup_oauth2 as oauth2;

use clap::{App, Arg, SubCommand};
use drive3::{Comment, DriveHub, Error, File, Result};
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
        .get_matches();

    let secret_file = matches
        .value_of("secret_file")
        .unwrap_or("/home/alan/.google-service-cli/drive3-secret.json");

    let target_dir = matches
        .value_of("target_dir")
        .unwrap_or("/tmp/pi_sync/images/");

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

fn get_folder_id(hub: &Hub, path: &str) -> std::result::Result<String, String> {
    let result = hub
        .files()
        .list()
        .corpora("user")
        .q("mimeType = 'application/vnd.google-apps.folder'")
        .q(format!("name='{}'", path).as_str())
        .doit();

    info!(log, "{:?}", result);

    Ok("19ipt2Rg1TGzr5esE_vA_1oFjrt7l5g7a".to_owned())
}

fn upload(hub: &Hub, path: &str) -> std::result::Result<u16, String> {
    let file_path = std::path::Path::new(path);
    let mut req = drive3::File::default();
    req.name = Some(file_path.file_name().unwrap().to_str().unwrap().to_string());
    req.parents = Some(vec![get_folder_id(&hub, "").unwrap()]);

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
            std::fs::File::open(file_path.to_str().unwrap()).unwrap(),
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
                error!(log, "Faile to invoke upload api {}", e);
                Err(e.to_string())
            }
        },
        Ok(res) => {
            info!(log, "Success Upload: {:?}", res);
            Ok(res.0.status.to_u16())
        }
    }
}

fn mkdir(hub: &Hub, path: &str) -> std::result::Result<u16, String> {
    let dir_path = std::path::Path::new(path);

    info!(log, "Mkdir:: Dir to create: {:?}", dir_path);

    let mut file = tempfile().unwrap();
    let mut req = drive3::File::default();
    req.parents = Some(vec![get_folder_id(&hub, "").unwrap()]);

    req.mime_type = Some("application/vnd.google-apps.folder".to_string());
    req.name = Some(dir_path.file_name().unwrap().to_str().unwrap().to_string());

    // Values shown here are possibly random and not representative !
    let result = hub
        .files()
        .create(req)
        .upload(file, "application/vnd.google-apps.folder".parse().unwrap());

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

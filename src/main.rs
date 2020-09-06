#[macro_use]
extern crate derive_new;
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

mod common;
mod drive_cli;
mod pi_err;
mod upload_handler;

use self::base64::encode;
use clap::{App, Arg};
use drive3::{Comment, DriveHub, Error, File, Result};
use drive_cli::{CloudClient, Drive3Client};
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
use upload_handler::{FileOperations, SyncableFile};

const PI_DRIVE_SYNC_PROPS_KEY: &str = "pi_sync_id";
const DIR_SCAN_DELAY: &str = "1";
const ROOT_FOLDER_ID: &str = "19ipt2Rg1TGzr5esE_vA_1oFjrt7l5g7a"; //TODO, needs to be smarter
const LOCAL_FILE_STORE: &str = "/tmp/pi_sync/images";
const DRIVE_ROOT_FOLDER: &str = "RpiCamSyncer";

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

    //Create Base Folder on Cloud Provider
    //make sure it exists locally too
    let syncer_drive_cli = drive_cli::Drive3Client::new(secret_file.to_owned());

    let root_remote_dir = format!(
        "{}/{}",
        upload_handler::LOCAL_ROOT_FOLDER,
        upload_handler::DRIVE_ROOT_FOLDER
    );

    std::fs::create_dir(root_remote_dir.clone());
    match syncer_drive_cli.id(&root_remote_dir) {
        Ok(id) => match id {
            Some(id) => debug!(log, "Root Dir Exists, not creating"),
            None => match syncer_drive_cli.create_dir(&root_remote_dir, None) {
                Ok(id) => debug!(log, "Created Root Dir {:?}", id),
                Err(e) => debug!(log, "Could not create root dir {:?}", e),
            },
        },
        Err(e) => warn!(log, "Error getting drive id for root folder"),
    }

    let (sender, receiver) = channel();
    let mut watcher =
        watcher(sender, Duration::from_secs(scan_interval_seconds)).expect("Cannot Create Watcher");
    watcher
        .watch(target_dir, RecursiveMode::Recursive)
        .expect("Canot Watch Dir");

    let handle_event = |_, p: std::path::PathBuf| {
        if let Some(path) = p.to_str() {
            let parent_path = SyncableFile::new(path.to_owned()).parent_path().unwrap();
            let parent_path = parent_path.to_str().unwrap();
            let parent_id = syncer_drive_cli.id(parent_path);
            trace!(log, "Parent Id for {:?}=  {:?}", p, parent_id);

            trace!(log, "Dir  Create {:?}", path);

            match parent_id {
                Ok(pid) => {
                    if SyncableFile::new(path.into()).is_dir() {
                        match syncer_drive_cli.create_dir(path, Some(pid.unwrap().as_str())) {
                            Ok(id) => debug!(log, "created {}, id = {:?}", path, id),
                            Err(e) => warn!(log, "cannot  create {} {}", path, e),
                        }
                    } else {
                        match syncer_drive_cli.upload_file(path, Some(pid.unwrap().as_str())) {
                            Ok(id) => debug!(log, "created {}, id = {:?}", path, id),
                            Err(e) => warn!(log, "cannot  create {} {}", path, e),
                        }
                    }
                }
                Err(e) => warn!(
                    log,
                    "cannot  fetch parent id, auth issues likely {} {}", path, e
                ),
            }
        } else {
            warn!(log, "Cannot Create {:?}", p);
        }
    };

    loop {
        //let hub = &hub;
        match receiver.recv() {
            Ok(notify::DebouncedEvent::Create(p)) => {
                handle_event("", p.clone());
            }
            _ => trace!(log, "unidentified event"),
            Err(e) => error!(log, "watch error: {:?}", e),
        }
    }
}

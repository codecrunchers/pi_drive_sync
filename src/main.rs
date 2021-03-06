#[macro_use]
extern crate derive_new;
#[macro_use]
extern crate slog;
extern crate base64;
extern crate google_drive3 as drive3;
extern crate hyper;
extern crate hyper_rustls;
extern crate lazy_static;
extern crate notify;
extern crate tempfile;
extern crate yup_oauth2 as oauth2;

use clap::{App, Arg};
use common::LOG as log;
use drive_cli::CloudClient;
use notify::{raw_watcher, RecommendedWatcher, RecursiveMode, Watcher};
use std::sync::mpsc::channel;
use std::time::Duration;
use upload_handler::{FileOperations, SyncableFile};

mod common;
mod drive_cli;
mod pi_err;
mod upload_handler;

fn main() {
    debug!(log, "Statring Syncer");

    let matches = App::new("Rusty Cam Syncer")
        .version("1.0")
        .author("Alan R. <alan@alanryan.name>")
        .version("1.0")
        .about("Will Sync a Dir recursvively with the smarts of a sheep")
        .arg(
            Arg::with_name("check_auth")
                .short("a")
                .long("check_auth")
                .value_name("check_auth")
                .help("yes/no")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("secret_file")
                .short("s")
                .long("secret_file")
                .value_name("secret_file")
                .help("Where to find Google Drive API JSON secrets")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("regexp_filter")
                .short("f")
                .long("regexp_filter")
                .value_name("regexp_filter")
                .help("Command separated list of filters, e.g. ^im.*jpg$ or ^vi.*mp4$ ")
                .takes_value(true),
        )
        .get_matches();

    let do_auth = matches.value_of("check_auth");

    let file_name_filters = matches
        .value_of("regexp_filter")
        .unwrap_or("")
        .split(",")
        .collect::<Vec<&str>>();

    let secret_file = matches
        .value_of("secret_file")
        .unwrap_or("/home/alan/.google-service-cli/drive3-secret.json");

    debug!(log, "Using {} as Auth File", secret_file);

    //Create Base Folder on Cloud Provider
    //make sure it exists locally too
    let syncer_drive_cli = drive_cli::Drive3Client::new(secret_file.to_owned(), file_name_filters);
    if let Err(hub_err) = syncer_drive_cli.get_hub() {
        println!("Error {}", hub_err);
        error!(log, "Cloud Provider {}", hub_err);
        std::process::exit(0x0100);
    }

    //create our dirs to sync
    let root_remote_dir = format!(
        "{}/{}",
        upload_handler::LOCAL_ROOT_FOLDER, //TODO: should be using target dir
        upload_handler::DRIVE_ROOT_FOLDER
    );
    let target_dir = root_remote_dir.clone();
    debug!(log, "Using {} as Local Dir to monitor", root_remote_dir);

    if let Err(e) = std::fs::create_dir(root_remote_dir.clone()) {
        warn!(log, "Root Folder Create Response: {}", e.to_string());
    }

    match syncer_drive_cli.id(&root_remote_dir) {
        Ok(id) => match id {
            Some(_id) => debug!(log, "Root Dir Exists, not creating"),
            None => match syncer_drive_cli.create_dir(&root_remote_dir, None) {
                Ok(id) => debug!(log, "Created Root Dir {:?}", id),
                Err(e) => debug!(log, "Could not create root dir {:?}", e),
            },
        },
        Err(_e) => warn!(log, "Error getting drive id for root folder"),
    }

    if let Some("yes") = matches.value_of("check_auth") {
        println!("Token Check Done");
        debug!(
            log,
            "outta here, just getting a token check - we may have never meet"
        );
        std::process::exit(0x0100);
    }

    let handle_event = |_, p: std::path::PathBuf| {
        if let Some(path) = p.to_str() {
            let file_to_sync = SyncableFile::new(path.into());
            if syncer_drive_cli.passes_filter(path) {
                if file_to_sync.is_file() {
                    match syncer_drive_cli.upload_file(path /*, Some(pid.unwrap().as_str())*/) {
                        Ok(id) => debug!(log, "created File {}, id = {:?}", path, id),
                        Err(e) => warn!(log, "cannot  create  File{} {}", path, e),
                    }
                } else {
                    info!(log, "Not creating dir {}", path);
                }
            } else {
                debug!(log, "{} is filtered out", path);
            }
        } else {
            warn!(log, "Cannot Create {:?}", p);
        }
    };

    let (sender, receiver) = channel();
    let mut watcher: RecommendedWatcher = Watcher::new_raw(sender).expect("cannot create watcher");
    watcher
        .watch(target_dir, RecursiveMode::Recursive)
        .expect("cannot create watcher");

    loop {
        match receiver.recv() {
            Ok(notify::RawEvent {
                path: Some(path),
                op: Ok(op),
                cookie,
            }) => {
                if op == notify::Op::CREATE {
                    trace!(log, "handled event {:?}{:?}{:?}", path, op, cookie);
                    handle_event("", path.clone());
                } else {
                    warn!(log, "unhandled event {:?}{:?}{:?}", path, op, cookie);
                }
            }
            Ok(other) => trace!(log, "unhandled event {:?}", other),
            Err(e) => error!(log, "watch error: {:?}", e),
        }
    }
}

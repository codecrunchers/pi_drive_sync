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

    let syncer_drive_cli = drive_cli::Drive3Client::new(secret_file.to_owned());

    //syncer_drive_cli.create_dir("RpiCamsyncer", None);

    /*let s = SyncableFile::new(
        format!("{}/{}", upload_handler::LOCAL_ROOT_FOLDER, "RpiSyncer1"),
        syncer_drive_cli,
    );
    s.upload();
    */

    let (sender, receiver) = channel();
    let mut watcher =
        watcher(sender, Duration::from_secs(scan_interval_seconds)).expect("Cannot Create Watcher");
    watcher
        .watch(target_dir, RecursiveMode::Recursive)
        .expect("Canot Watch Dir");

    let handle_event = |_h, p: std::path::PathBuf| {
        if let Some(path) = p.to_str() {
            if std::path::Path::new(path).is_dir() {
                trace!(log, "Dir  Create {:?}", p);
            //mkdir(&hub, path).unwrap_or({
            //  warn!(log, "Cannot create dir");
            //  0
            //});
            } else {
                trace!(log, "File Upload {:?}", p);
                /*                upload(&hub, path).unwrap_or({
                                    warn!(log, "Cannot create file");
                                    0
                                });
                */
            }
        } else {
            warn!(log, "Cannot Create {:?}", p);
        }
    };

    loop {
        //let hub = &hub;
        match receiver.recv() {
            Ok(notify::DebouncedEvent::Create(p)) => {
                handle_event("s", p.clone());
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
        let base64_buf = encode(format!("{}{}", DRIVE_ROOT_FOLDER, &file_name));
        trace!(log,"get_unique_entry_id as b64 of {}{} = {:?}", DRIVE_ROOT_FOLDER, &file_name, base64_buf; "x"=>1);
        Some(base64_buf.clone())
    })
}

fn strip_local_fs(lfsn: &str) -> Option<&str> {
    trace!(log,"strip_local_fs"; "lfsn"=>lfsn);
    if lfsn.len() > LOCAL_FILE_STORE.len() && lfsn[0..LOCAL_FILE_STORE.len()].eq(LOCAL_FILE_STORE) {
        Some(&lfsn[LOCAL_FILE_STORE.len() + 1..])
    } else {
        trace!(log, "Invalid file {}, returning root of FileSystem", &lfsn);
        std::path::Path::new(lfsn).file_name()?.to_str()
    }
}

fn get_file_name(lfsn: &str) -> Option<&str> {
    trace!(log,"get_file_name"; "name"=>lfsn);
    let path = std::path::Path::new(lfsn);
    path.file_name()?.to_str()
}

/*fn parent_path_to_base_64(local_path: &str) -> Option<String> {
    trace!(log, "Path to id for {}", local_path);

    let ancestors = Path::new(local_path)
        .strip_prefix(Path::new(LOCAL_FILE_STORE))
        .and_then(|p| Ok(p.ancestors().next()))
        .ok();

    trace!(log, "ancestors={:?}", &ancestors,);
    ancestors.and_then(|a| get_unique_entry_id(a.unwrap().file_name()?.to_str()?))
}

fn local_dir_to_drive_file_id(hub: &Hub, path: &str) -> Option<String> {
    //hyper::client::Response, drive3::DriveList)> {
    trace!(log, "File Search for"; "path"=>path);

    let b64_id = parent_path_to_base_64(path)
        .unwrap_or(ROOT_FOLDER_ID.to_owned()) //here we use root
        .as_str()
        .to_owned();

    trace!(log, "B64 id  = {} for path {}", b64_id, path);

    let q = &format!(
        "{} {{ key='{}' and value='{}' }}",
        "appProperties has ", PI_DRIVE_SYNC_PROPS_KEY, b64_id
    );

    trace!(log, "Query {:?}", q);

    let result = hub.files().list().q(q).doit();

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
                None
            }
        },
        Ok(res) => {
            trace!(log, "Query Success {:?}", res);
            res.1.files?.get(0)?.id.clone()
        }
    }
}

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

    if path.ne(DRIVE_ROOT_FOLDER) {
        req.parents = Some(vec![
            local_dir_to_drive_file_id(&hub, &path).unwrap_or("".to_owned())
        ])
    }

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
/// 2020 -> [01,02,...12] [m]: n=>{1..31}, [d] [0-7]...
fn mkdir(hub: &Hub, path: &str) -> std::result::Result<u16, String> {
    trace!(log, "Mkdir:: Dir to create: {:?}", path);
    let mut temp_file = tempfile().expect("err");
    let mut req = drive3::File::default();

    req.name = get_file_name(path).and_then(|p| Some(p.into()));
    trace!(log, "File name {:?}", req.name);

    if path.ne(DRIVE_ROOT_FOLDER) {
        req.parents = Some(vec![local_dir_to_drive_file_id(&hub, &path)
            .unwrap_or("1iwbQaaWNQgWYxGI4NSjrOzfDtbRrnc_o".to_owned())])
    }

    get_unique_entry_id(path).and_then(|b64_id| {
        trace!(log, "Added Drive Id"; "id"=>b64_id.clone());
        Some(req.app_properties = app_props_map(&b64_id))
    });

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
                Err(e.to_string())
            }
        },
        Ok(res) => {
            trace!(log, "Success, dir  created: {:?}", res);
            Ok(res.0.status.to_u16())
        }
    }
}*/

#[cfg(test)]
mod tests {
    use crate::common::LOG as log;
}

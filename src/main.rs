extern crate hyper;
extern crate hyper_rustls;
extern crate yup_oauth2 as oauth2;
extern crate google_drive3 as drive3;
extern crate tempfile;
extern crate notify;
use notify::{RecommendedWatcher, RecursiveMode, Result, watcher};
use std::time::Duration;
use tempfile::tempfile;
use drive3::{Result, Error};
use drive3::File;
use std::fs;
use drive3::Comment;
use std::default::Default;
use oauth2::{Authenticator, DefaultAuthenticatorDelegate, ApplicationSecret, MemoryStorage,  read_application_secret, parse_application_secret ,DiskTokenStorage};
use drive3::DriveHub;
use std::io::prelude::*;

type Hub =  drive3::DriveHub<hyper::Client, oauth2::Authenticator<oauth2::DefaultAuthenticatorDelegate, oauth2::DiskTokenStorage, hyper::Client>>;



fn read_client_secret(file: String) -> ApplicationSecret {
    read_application_secret(std::path::Path::new(&file)).unwrap()
        //        parse_application_secret(&file.to_string()).unwrap()

}

const CLIENT_SECRET_FILE: &'static str = "/home/alan/.google-service-cli/drive3-secret.json";

fn main() {
    println!("Hello, world!");

    let secret = read_client_secret(CLIENT_SECRET_FILE.to_string());

    let token_storage = DiskTokenStorage::new(&String::from("temp_token")).unwrap();
    let mut auth = Authenticator::new(&secret, DefaultAuthenticatorDelegate,
        hyper::Client::with_connector(hyper::net::HttpsConnector::new(hyper_rustls::TlsClient::new())),
        token_storage, Some(yup_oauth2::FlowType::InstalledInteractive));

    let mut hub = DriveHub::new(hyper::Client::with_connector(hyper::net::HttpsConnector::new(hyper_rustls::TlsClient::new())), auth);

    let mut inotify = Inotify::init()
        .expect("Failed to initialize inotify");


    inotify.add_watch(
        "/tmp/camera",
        WatchMask::MODIFY | WatchMask::CREATE | WatchMask::DELETE,
    )
        .expect("Failed to add inotify watch");



    println!("Watching current directory for activity...");

    let mut buffer = [0u8; 4096];
    loop {
        let events = inotify
            .read_events_blocking(&mut buffer)
            .expect("Failed to read inotify events");

        for event in events {
            if event.mask.contains(EventMask::CREATE) {
                if event.mask.contains(EventMask::ISDIR) {
                    println!("Directory created: {:?}, {:?}", event.name);
                    inotify.add_watch(
                        format!("{}/{}/{}", "/tmp","camera",event.name.unwrap().to_str().unwrap().to_string()),
                        WatchMask::MODIFY | WatchMask::CREATE | WatchMask::DELETE,
                    )
                        .expect("Failed to add inotify watch");

                    } else {
                        println!("File created: {:?}", event.name);
                }
            } else if event.mask.contains(EventMask::DELETE) {
                if event.mask.contains(EventMask::ISDIR) {
                    println!("Directory deleted: {:?}", event.name);
                } else {
                    println!("File deleted: {:?}", event.name);
                }
            } else if event.mask.contains(EventMask::MODIFY) {
                if event.mask.contains(EventMask::ISDIR) {
                    println!("Directory modified: {:?}", event.name);
                } else {
                    println!("File modified: {:?}", event.name);
                }
            }
        }
    }
}



fn upload(hub: Hub,  path: &str){
    let file_path = std::path::Path::new(path);
    let mut req = drive3::File::default();
    req.name= Some(file_path.file_name().unwrap().to_str().unwrap().to_string());

    // Values shown here are possibly random and not representative !
    /*    let result = hub.files().create(req)
          .use_content_as_indexable_text(true)
          .supports_team_drives(false)
          .supports_all_drives(true)
          .keep_revision_forever(false)
          .ignore_default_visibility(true)
          .enforce_single_parent(true)
          .upload_resumable(fs::File::open(file_path.to_str().unwrap()).unwrap(), "application/octet-stream".parse().unwrap());*/

}

fn mkdir(hub: Hub, path: &str) -> std::result::Result<u32, String> {
    let dir_path = std::path::Path::new(path);

    let mut req = drive3::File::default();

    req.mime_type = Some("application/vnd.google-apps.folder".to_string());
    req.name= Some(dir_path.file_name().unwrap().to_str().unwrap().to_string());

    // Values shown here are possibly random and not representative !
    /*let result = hub.files().create(req);

      match result {
      Err(e) => match e {
    // The Error enum provides details about what exactly happened.
    // You can also just use its `Debug`, `Display` or `Error` traits
    Error::HttpError(_)
    |Error::MissingAPIKey
    |Error::MissingToken(_)
    |Error::Cancelled
    |Error::UploadSizeLimitExceeded(_, _)
    |Error::Failure(_)
    |Error::BadRequest(_)
    |Error::FieldClash(_)
    |Error::JsonDecodeError(_, _) => {
    println!("{}", e);
    e.to_string()
    }
    },
    Ok(res) => {
    println!("Success: {:?}", res);
    res.status()
    }
    }*/
    Ok(200)
}



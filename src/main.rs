extern crate hyper;
extern crate hyper_rustls;
extern crate yup_oauth2 as oauth2;
extern crate google_drive3 as drive3;
extern crate tempfile;
extern crate notify;
use notify::{Watcher, RecursiveMode, watcher};
use std::sync::mpsc::channel;
use std::path::Path;
use drive3::{
    Result, 
    Error, 
    File, 
    Comment, 
    DriveHub
};
use std::time::Duration;
use tempfile::tempfile;
use std::default::Default;
use oauth2::{Authenticator, DefaultAuthenticatorDelegate, ApplicationSecret, MemoryStorage,  read_application_secret, parse_application_secret ,DiskTokenStorage};

type Hub =  drive3::DriveHub<hyper::Client, oauth2::Authenticator<oauth2::DefaultAuthenticatorDelegate, oauth2::DiskTokenStorage, hyper::Client>>;



fn read_client_secret(file: String) -> ApplicationSecret {
    read_application_secret(std::path::Path::new(&file)).unwrap()

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

    // Values shown here are possibly random and not representative !
let result = hub.files().list()
             .doit();

    println!("{:?}", result);


    // Create a channel to receive the events.
    let (sender, receiver) = channel();

    // Create a watcher object, delivering debounced events.
    // The notification back-end is selected based on the platform.
    let mut watcher = watcher(sender, Duration::from_secs(10)).unwrap();

    // Add a path to be watched. All files and directories at that path and
    // below will be monitored for changes.
    watcher.watch("/tmp/camera", RecursiveMode::Recursive).unwrap();

    loop {
        match receiver.recv() {
            Ok(event) => {
                match event {
                    notify::DebouncedEvent::Create(p) => {
                        println!("{:?}",p);
                        let path = p.to_str().unwrap();
                        if std::path::Path::new(path).is_dir() {
                            mkdir(&hub, path);
                        }else{
                            upload(&hub, path);
                        }


                    }

                    _ =>  println!("err"),
                };
                //println!("{:?}", event);
            }
            Err(e) => println!("watch error: {:?}", e),
        }
    }
}



fn upload(hub: &Hub,  path: &str)  -> std::result::Result<u16, String> {
    let file_path = std::path::Path::new(path);
    let mut req = drive3::File::default();
    req.name= Some(file_path.file_name().unwrap().to_str().unwrap().to_string());

    // Values shown here are possibly random and not representative !
    let result = hub.files().create(req)
        .use_content_as_indexable_text(true)
        .supports_team_drives(false)
        .supports_all_drives(true)
        .keep_revision_forever(false)
        .ignore_default_visibility(true)
        .enforce_single_parent(true)
        .upload_resumable(std::fs::File::open(file_path.to_str().unwrap()).unwrap(), "application/octet-stream".parse().unwrap());

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
                    Err(e.to_string())
                }
        },
        Ok(res) => {
            println!("Success: {:?}", res);
            Ok(res.0.status.to_u16())
        }
    }


}

fn mkdir(hub: &Hub, path: &str) -> std::result::Result<u16, String> {
    let dir_path = std::path::Path::new(path);

    let mut file = tempfile().unwrap();
    let mut req = drive3::File::default();

    req.mime_type = Some("application/vnd.google-apps.folder".to_string());
    req.name= Some(format!("{}/{}", "camera", dir_path.file_name().unwrap().to_str().unwrap().to_string()));

    // Values shown here are possibly random and not representative !
    let result = hub.files().create(req).upload(file, "application/vnd.google-apps.folder".parse().unwrap());

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

                    Err(e.to_string())
                }
        },
        Ok(res) => {
            println!("Success: {:?}", res);
            Ok(res.0.status.to_u16())
        }
    }

}



use drive3::{Comment, DriveHub, Error, File, Result};
use notify::{watcher, RecursiveMode, Watcher};
use oauth2::{
    parse_application_secret, read_application_secret, ApplicationSecret, Authenticator,
    DefaultAuthenticatorDelegate, DiskTokenStorage, MemoryStorage,
};
use std::collections::HashMap;
use std::default::Default;
use std::sync::mpsc::channel;
use std::time::Duration;
use tempfile::tempfile;

type Hub = drive3::DriveHub<
    hyper::Client,
    oauth2::Authenticator<
        oauth2::DefaultAuthenticatorDelegate,
        oauth2::DiskTokenStorage,
        hyper::Client,
    >,
>;

struct Drive3Client {}
trait CloudClient {
    fn upload_file(local_fs_path: &std::path::Path, parent_id: &str) -> String;
    fn create_dir(local_fs_path: &std::path::Path, parent_id: &str) -> String;
    fn drive_id_from_pi_sync_id(pi_sync: &str) -> String;
}

impl CloudClient for Drive3Client {
    fn upload_file(local_fs_path: &std::path::Path, parent_id: &str) -> String {
        todo!()
    }
    fn create_dir(local_fs_path: &std::path::Path, parent_id: &str) -> String {
        todo!()
    }
    fn drive_id_from_pi_sync_id(pi_sync: &str) -> String {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use crate::common::LOG as log;

    #[test]
    fn test_strip_local_fs() {
        assert_eq!(1, 2);
    }
}

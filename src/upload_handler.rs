use crate::pi_err::SyncerErrors;
use base64::{decode, encode};
use std::path::{Path, PathBuf, StripPrefixError};

const PI_DRIVE_SYNC_PROPS_KEY: &str = "pi_sync_id";
const DIR_SCAN_DELAY: &str = "1";
const ROOT_FOLDER_ID: &str = "19ipt2Rg1TGzr5esE_vA_1oFjrt7l5g7a"; //TODO, needs to be smarter
const LOCAL_ROOT_FOLDER: &str = "/tmp/pi_sync/images";
const DRIVE_ROOT_FOLDER: &str = "RpiCamSyncer";

#[derive(new)]
struct Syncable {
    local_disk_path: String,
}

trait Uploader {
    fn upload(&self) -> Option<String>;
    fn local_path(&self) -> &Path;
    fn cloud_path(&self) -> Result<PathBuf, SyncerErrors>;
    fn parent_path(&self) -> Result<PathBuf, SyncerErrors>;
    fn is_file(&self) -> bool;
    fn is_dir(&self) -> bool;
    fn get_unique_id(&self) -> Result<String, SyncerErrors>;
    fn search_remote_store_for_unique_id(&self) -> Option<String>;
}

impl Uploader for Syncable {
    fn upload(&self) -> Option<String> {
        todo!()
    }
    fn local_path(&self) -> &Path {
        Path::new(&self.local_disk_path)
    }
    fn cloud_path(&self) -> Result<PathBuf, SyncerErrors> {
        Ok(Path::new(DRIVE_ROOT_FOLDER).join(
            self.local_path()
                .strip_prefix(Path::new(LOCAL_ROOT_FOLDER))?,
        ))
    }

    fn parent_path(&self) -> Result<PathBuf, SyncerErrors> {
        let mut p_copy = PathBuf::from(&self.cloud_path().unwrap());
        match p_copy.pop() {
            true => Ok(p_copy),
            false => Err(SyncerErrors::InvalidPathError),
        }
    }

    fn is_file(&self) -> bool {
        self.local_path().is_file()
    }
    fn is_dir(&self) -> bool {
        self.local_path().is_dir()
    }
    fn get_unique_id(&self) -> Result<String, SyncerErrors> {
        let cp = &self.cloud_path()?;
        cp.to_str()
            .and_then(|p| Some(Ok(encode(p))))
            .ok_or(SyncerErrors::SyncerNoneError)?
    }
    fn search_remote_store_for_unique_id(&self) -> Option<String> {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use crate::common::LOG as log;
    use crate::upload_handler::*;

    #[test]
    fn test_upload_get_unique_id_file() {
        let local_file = format!("{}{}", LOCAL_ROOT_FOLDER, "/alan.txt");
        let s = Syncable::new(local_file.to_owned());

        let cp = s.cloud_path();
        assert_eq!(
            format!("{}/{}", DRIVE_ROOT_FOLDER, "alan.txt"),
            cp.unwrap().to_str().unwrap()
        );
        assert_eq!(
            encode(format!("{}/{}", DRIVE_ROOT_FOLDER, "alan.txt")),
            s.get_unique_id().unwrap()
        );
    }

    #[test]
    fn test_upload_get_unique_id_dir() {
        let local_file = format!("{}{}", LOCAL_ROOT_FOLDER, "/alan");
        let s = Syncable::new(local_file.to_owned());

        let cp = s.cloud_path();
        assert_eq!(
            format!("{}/{}", DRIVE_ROOT_FOLDER, "alan"),
            cp.unwrap().to_str().unwrap()
        );
        assert_eq!(
            encode(format!("{}/{}", DRIVE_ROOT_FOLDER, "alan")),
            s.get_unique_id().unwrap()
        );
    }

    #[test]
    fn test_upload_is_file() {
        let local_file = format!("{}{}", LOCAL_ROOT_FOLDER, "/alan.txt");
        let s = Syncable::new(local_file.to_owned());
        assert_eq!(true, s.is_file());
    }

    #[test]
    fn test_upload_is_dir() {
        let local_dir = format!("{}{}", LOCAL_ROOT_FOLDER, "/alan");
        let s = Syncable::new(local_dir.to_owned());
        assert_eq!(true, s.is_dir());
    }

    #[test]
    fn test_upload_remote_path() {
        let local_dir = format!("{}{}", LOCAL_ROOT_FOLDER, "/alan");
        let s = Syncable::new(local_dir.clone().to_owned());
        let rp = Path::new(DRIVE_ROOT_FOLDER).join("alan");
        let cp = s.cloud_path();
        assert_eq!(rp, cp.unwrap());
    }

    #[test]
    fn test_upload_local_path() {
        let local_dir = format!("{}{}", LOCAL_ROOT_FOLDER, "/alan");
        let s = Syncable::new(local_dir.clone().to_owned());
        let p = Path::new(&local_dir);
        let lp = s.local_path();
        assert_eq!(p, lp);
    }

    #[test]
    fn test_uploader() {
        let s = Syncable::new(format!("{}{}", LOCAL_ROOT_FOLDER, "/alan"));
        let id = s.upload();
        assert_eq!("".to_owned(), id.unwrap());
    }

    #[test]
    fn test_upload_parent_path() {
        let child = format!("{}{}", LOCAL_ROOT_FOLDER, "/a/a.txt");
        let c = Syncable::new(child.to_owned());
        assert_eq!(
            Path::new(DRIVE_ROOT_FOLDER).join("a"),
            c.parent_path().unwrap()
        );

        let child1 = format!("{}{}", LOCAL_ROOT_FOLDER, "/b/b");
        let c1 = Syncable::new(child1.to_owned());
        assert_eq!(
            Path::new(DRIVE_ROOT_FOLDER).join("b"),
            c1.parent_path().unwrap()
        );

        let child2 = format!("{}{}", LOCAL_ROOT_FOLDER, "/c/c/test.txt");
        let c2 = Syncable::new(child2.to_owned());
        assert_eq!(
            Path::new(DRIVE_ROOT_FOLDER).join("c/c"),
            c2.parent_path().unwrap()
        );

        let child3 = format!("{}{}", LOCAL_ROOT_FOLDER, "/d");
        let c3 = Syncable::new(child3.to_owned());
        assert_eq!(Path::new(DRIVE_ROOT_FOLDER), c3.parent_path().unwrap());
    }

    #[test]
    fn test_upload_generate_parent_unique_id() {
        let parent = format!("{}{}", LOCAL_ROOT_FOLDER, "/alan");
        let f = Syncable::new(parent.clone().to_owned());
        let folder_id = f.get_unique_id();

        let child = format!("{}{}", LOCAL_ROOT_FOLDER, "/alan/alan.txt");
        let c = Syncable::new(child.to_owned());

        let rp = Path::new(DRIVE_ROOT_FOLDER).join("alan/alan.txt");
        assert_eq!(rp.as_path(), c.cloud_path().unwrap());

        let child_parent_path = c.parent_path();
        let ntpath = child_parent_path.unwrap().to_str().unwrap().to_owned();
        println!("{}", ntpath);
        assert_eq!(encode(ntpath), folder_id.unwrap());
    }

    #[test]
    fn test_search_remote_store_for_unique_id() {
        let s = Syncable::new(format!("{}{}", LOCAL_ROOT_FOLDER, "/alan"));
        let id = s.search_remote_store_for_unique_id();
        assert!(false);
    }
}

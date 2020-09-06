use crate::common::LOG as log;
use crate::drive_cli::{CloudClient, Drive3Client, Hub};
use crate::pi_err::{PiSyncResult, SyncerErrors};
use base64::{decode, encode};
use std::io::prelude::*;
use std::path::{Path, PathBuf, StripPrefixError};

const DIR_SCAN_DELAY: &str = "1";
pub const LOCAL_ROOT_FOLDER: &str = "/tmp/pi_sync/images"; //basing base64 on this is dodgy as if I change this we get a different id
pub const DRIVE_ROOT_FOLDER: &str = "RpiCamera";

#[derive(new)]
pub struct SyncableFile {
    local_disk_path: String,
}

pub trait FileOperations {
    ///The path to the file on disk
    fn local_path(&self) -> &Path;
    ///Where on the cloud srorage provider is this file dir to be created
    fn cloud_path(&self) -> PiSyncResult<PathBuf>;
    ///return the ancestors path on local disk e.g /tmp/images/a will return /tmp/images/
    fn parent_path(&self) -> PiSyncResult<PathBuf>;
    ///Is this a file on disk
    fn is_file(&self) -> bool;
    ///Is this a directory on disk
    fn is_dir(&self) -> bool;
    ///Take the  unique cloud porition of the file
    ///path and return a Base64 Unique Id of this
    fn get_unique_id(&self) -> PiSyncResult<String>;
}

impl FileOperations for SyncableFile {
    fn local_path(&self) -> &Path {
        Path::new(&self.local_disk_path)
    }
    fn cloud_path(&self) -> PiSyncResult<PathBuf> {
        Ok(Path::new(DRIVE_ROOT_FOLDER).join(
            self.local_path()
                .strip_prefix(Path::new(LOCAL_ROOT_FOLDER))?,
        ))
    }

    ///Using the local fs based path, return the parent directory path
    fn parent_path(&self) -> PiSyncResult<PathBuf> {
        let mut p_copy = PathBuf::from(&self.local_disk_path);
        match p_copy.pop() {
            true => {
                debug!(log, "FileOperations::Parent Path = {:?}", p_copy);
                Ok(p_copy)
            }
            false => {
                error!(log, "cannot pop  {:?}", p_copy);
                //Err(SyncerErrors::InvalidPathError("Cannot calc parent path"))
                Err(SyncerErrors::InvalidPathError)
            }
        }
    }

    fn is_file(&self) -> bool {
        self.local_path().is_file()
    }

    fn is_dir(&self) -> bool {
        self.local_path().is_dir()
    }

    ///Return a Base64 representation of the file path on your storage host
    fn get_unique_id(&self) -> PiSyncResult<String> {
        let cp = &self.cloud_path()?;
        cp.to_str()
            .and_then(|p| Some(Ok(encode(p))))
            .ok_or(SyncerErrors::SyncerNoneError)?
    }
}

#[cfg(test)]
mod tests {
    use crate::common::LOG as log;
    use crate::drive_cli::*;
    use crate::upload_handler::*;
    use std::path::{Path, PathBuf, StripPrefixError};

    fn syncable_file(p: String) -> SyncableFile {
        SyncableFile::new(p)
    }

    #[test]
    fn test_upload_get_unique_id_file() {
        let local_file = format!("{}{}", LOCAL_ROOT_FOLDER, "/alan.txt");
        let s = syncable_file(local_file);
        let cp = s.cloud_path().unwrap();
        let cp_string = cp.to_str().unwrap();

        assert_eq!(
            format!("{}/{}", DRIVE_ROOT_FOLDER, "alan.txt"),
            cp_string,
            "Cloud path not correctly computed"
        );
        assert_eq!(
            encode(format!("{}/{}", DRIVE_ROOT_FOLDER, "alan.txt")),
            s.get_unique_id().unwrap(),
            "Base64 Calc of Syncable File does not match a manual encode of same path"
        );
    }

    #[test]
    fn test_upload_get_unique_id_dir() {
        let local_dir_parent = format!("{}{}", LOCAL_ROOT_FOLDER, "/alan");
        let parent_dir = syncable_file(local_dir_parent.clone());
        assert_eq!(local_dir_parent, parent_dir.local_path().to_str().unwrap());
        let puid = parent_dir.get_unique_id().unwrap();

        let local_file = format!("{}{}", LOCAL_ROOT_FOLDER, "/alan/alan.txt");
        let child_file = syncable_file(local_file);

        let pp_from_child_path = child_file.parent_path().unwrap();
        let parent_path_as_string = pp_from_child_path.to_str().unwrap();
        assert_eq!(
            "/tmp/pi_sync/images/alan", parent_path_as_string,
            "Parent Path Calc is wrong"
        );

        //issue cannot construct a Sycable path from Cloud Path

        let tmp_syncable = SyncableFile::new(parent_path_as_string.clone().to_owned());
        let tuid = tmp_syncable.get_unique_id().unwrap();
        assert_eq!(puid, tuid);
    }

    #[test]
    fn test_upload_is_file() {
        let mut file = std::fs::File::create("/tmp/pi_sync/images/alan.txt").unwrap();
        file.write_all(b"empty_file\n").unwrap();

        let local_file = format!("{}{}", LOCAL_ROOT_FOLDER, "/alan.txt");
        let s = syncable_file(local_file);
        assert_eq!(true, s.is_file());
    }

    #[test]
    fn test_upload_is_dir() {
        std::fs::create_dir("/tmp/pi_sync/images/alan");
        let local_dir = format!("{}{}", LOCAL_ROOT_FOLDER, "/alan");
        let s = syncable_file(local_dir);
        assert_eq!(true, s.is_dir());
    }

    #[test]
    fn test_upload_remote_path() {
        let local_dir = format!("{}{}", LOCAL_ROOT_FOLDER, "/alan");
        let s = syncable_file(local_dir);
        let rp = Path::new(DRIVE_ROOT_FOLDER).join("alan");
        let cp = s.cloud_path();
        assert_eq!(rp, cp.unwrap());
    }

    #[test]
    fn test_upload_local_path() {
        let local_dir = format!("{}{}", LOCAL_ROOT_FOLDER, "/alan");
        let s = syncable_file(local_dir.clone());
        let lp = s.local_path();
        let str_lp = lp.to_str().unwrap();
        assert_eq!(local_dir, str_lp);
    }

    #[test]
    fn test_upload_parent_path() {
        let root_file = format!("{}{}", LOCAL_ROOT_FOLDER, "/alan.txt");
        let root_s = syncable_file(root_file);

        assert_eq!(
            Path::new(LOCAL_ROOT_FOLDER),
            root_s.parent_path().unwrap(),
            "Parent path is not correct"
        );

        let child = format!("{}{}", LOCAL_ROOT_FOLDER, "/a/a.txt");
        let c = syncable_file(child);

        assert_eq!(
            Path::new(LOCAL_ROOT_FOLDER).join("a"),
            c.parent_path().unwrap(),
            "Parent path is not correct for /a/a.txt"
        );

        let child1 = format!("{}{}", LOCAL_ROOT_FOLDER, "/b/b");
        let c1 = syncable_file(child1);

        assert_eq!(
            Path::new(LOCAL_ROOT_FOLDER).join("b"),
            c1.parent_path().unwrap(),
            "Parent path is not correct for /b/b"
        );

        let child2 = format!("{}{}", LOCAL_ROOT_FOLDER, "/c/c/test.txt");
        let c2 = syncable_file(child2);
        assert_eq!(
            Path::new(LOCAL_ROOT_FOLDER).join("c/c"),
            c2.parent_path().unwrap(),
            "Parent path is not correct for /c/c/test.txt"
        );

        let child3 = format!("{}{}", LOCAL_ROOT_FOLDER, "/d");
        let c3 = syncable_file(child3);
        assert_eq!(
            Path::new(LOCAL_ROOT_FOLDER),
            c3.parent_path().unwrap(),
            "Parent path is not correct for /d"
        );
    }
}

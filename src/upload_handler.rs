use crate::drive_cli::{CloudClient, Drive3Client, Hub};

use crate::pi_err::SyncerErrors;

use crate::common::LOG as log;
use base64::{decode, encode};
use std::path::{Path, PathBuf, StripPrefixError};

const DIR_SCAN_DELAY: &str = "1";
const ROOT_FOLDER_ID: &str = "19ipt2Rg1TGzr5esE_vA_1oFjrt7l5g7a"; //TODO, needs to be smarter
pub const LOCAL_ROOT_FOLDER: &str = "/tmp/pi_sync/images";
pub const DRIVE_ROOT_FOLDER: &str = "RpiCamera";

#[derive(new)]
pub struct SyncableFile {
    local_disk_path: String,
}

pub trait FileOperations {
    fn local_path(&self) -> &Path;
    fn cloud_path(&self) -> Result<PathBuf, SyncerErrors>;
    fn parent_path(&self) -> Result<PathBuf, SyncerErrors>;
    fn is_file(&self) -> bool;
    fn is_dir(&self) -> bool;
    fn get_unique_id(&self) -> Result<String, SyncerErrors>;
}

impl FileOperations for SyncableFile {
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
        let mut p_copy = PathBuf::from(&self.cloud_path()?);
        match p_copy.pop() {
            true => {
                debug!(log, "FileOperations::Parent Path = {:?}", p_copy);
                Ok(PathBuf::from(format!(
                    "{}/{}",
                    LOCAL_ROOT_FOLDER,
                    p_copy.to_str().unwrap()
                )))
            }
            false => {
                error!(log, "cannot pop  {:?}", p_copy);
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
    fn get_unique_id(&self) -> Result<String, SyncerErrors> {
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

        println!("Root Test {}", cp_string);
        assert_eq!(format!("{}/{}", DRIVE_ROOT_FOLDER, "alan.txt"), cp_string);
        assert_eq!(
            encode(format!("{}/{}", DRIVE_ROOT_FOLDER, "alan.txt")),
            s.get_unique_id().unwrap()
        );
    }

    #[test]
    fn test_upload_get_unique_id_dir() {
        let local_file = format!("{}{}", LOCAL_ROOT_FOLDER, "/alan");
        let s = syncable_file(local_file);

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
        let s = syncable_file(local_file);
        assert_eq!(true, s.is_file());
    }

    #[test]
    fn test_upload_is_dir() {
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
        let p = Path::new(&local_dir);
        let lp = s.local_path();
        assert_eq!(p, lp);
    }

    #[test]
    fn test_upload_parent_path() {
        let root_file = format!("{}{}", LOCAL_ROOT_FOLDER, "/alan.txt");
        let root_s = syncable_file(root_file);

        assert_eq!(Path::new(DRIVE_ROOT_FOLDER), root_s.parent_path().unwrap());

        let child = format!("{}{}", LOCAL_ROOT_FOLDER, "/a/a.txt");
        let c = syncable_file(child);

        assert_eq!(
            Path::new(DRIVE_ROOT_FOLDER).join("a"),
            c.parent_path().unwrap()
        );

        let child1 = format!("{}{}", LOCAL_ROOT_FOLDER, "/b/b");
        let c1 = syncable_file(child1);

        assert_eq!(
            Path::new(DRIVE_ROOT_FOLDER).join("b"),
            c1.parent_path().unwrap()
        );

        let child2 = format!("{}{}", LOCAL_ROOT_FOLDER, "/c/c/test.txt");
        let c2 = syncable_file(child2);
        assert_eq!(
            Path::new(DRIVE_ROOT_FOLDER).join("c/c"),
            c2.parent_path().unwrap()
        );

        let child3 = format!("{}{}", LOCAL_ROOT_FOLDER, "/d");
        let c3 = syncable_file(child3);
        assert_eq!(Path::new(DRIVE_ROOT_FOLDER), c3.parent_path().unwrap());
    }

    #[test]
    fn test_upload_generate_parent_unique_id() {
        let parent = format!("{}{}", LOCAL_ROOT_FOLDER, "/alan");
        let f = syncable_file(parent);

        let folder_id = f.get_unique_id();

        let child = format!("{}{}", LOCAL_ROOT_FOLDER, "/alan/alan.txt");
        let c = syncable_file(child);
        let rp = Path::new(DRIVE_ROOT_FOLDER).join("alan/alan.txt");
        assert_eq!(rp.as_path(), c.cloud_path().unwrap());

        let child_parent_path = c.parent_path();
        let ntpath = child_parent_path.unwrap().to_str().unwrap().to_owned();
        assert_eq!(encode(ntpath), folder_id.unwrap());
    }
}

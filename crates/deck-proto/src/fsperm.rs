//! File permissions that exist only on Unix. Std only.
//!
//! On Unix the daemon keeps its files at 0600 and its folder at 0700. Windows
//! has no mode bits: there the files lie under `%LOCALAPPDATA%` or the profile
//! folder, whose access list already admits only the user, so these calls do
//! nothing.

use std::fs::{File, Metadata, OpenOptions};
use std::io;
use std::path::Path;

/// Files the daemon writes: only the user reads them.
pub const PRIVATE_FILE: u32 = 0o600;
/// The data folder: only the user enters it.
pub const PRIVATE_DIR: u32 = 0o700;

/// Create new files with `mode` (Unix only).
pub fn create_mode(opts: &mut OpenOptions, mode: u32) -> &mut OpenOptions {
    #[cfg(unix)]
    {
        std::os::unix::fs::OpenOptionsExt::mode(opts, mode);
    }
    #[cfg(not(unix))]
    let _ = mode;
    opts
}

/// Set `mode` on an open file, also when it existed before (Unix only).
pub fn set_mode(file: &File, mode: u32) -> io::Result<()> {
    #[cfg(unix)]
    {
        file.set_permissions(std::os::unix::fs::PermissionsExt::from_mode(mode))
    }
    #[cfg(not(unix))]
    {
        let _ = (file, mode);
        Ok(())
    }
}

/// Set `mode` on a path (Unix only).
pub fn set_path_mode(path: &Path, mode: u32) -> io::Result<()> {
    #[cfg(unix)]
    {
        std::fs::set_permissions(path, std::os::unix::fs::PermissionsExt::from_mode(mode))
    }
    #[cfg(not(unix))]
    {
        let _ = (path, mode);
        Ok(())
    }
}

/// The permission bits of a file, `None` where there are none.
pub fn mode(meta: &Metadata) -> Option<u32> {
    #[cfg(unix)]
    {
        Some(std::os::unix::fs::PermissionsExt::mode(&meta.permissions()) & 0o777)
    }
    #[cfg(not(unix))]
    {
        let _ = meta;
        None
    }
}

/// A file that can run: a file with an execute bit on Unix, any file on
/// Windows (where the extension decides).
pub fn is_executable(meta: &Metadata) -> bool {
    meta.is_file() && mode(meta).is_none_or(|m| m & 0o111 != 0)
}

/// Create the folder and its parents; the last one gets `mode` (Unix only).
pub fn create_dir(path: &Path, mode: u32) -> io::Result<()> {
    let mut b = std::fs::DirBuilder::new();
    b.recursive(true);
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut b, mode);
    #[cfg(not(unix))]
    let _ = mode;
    b.create(path)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn new_files_and_folders_are_private() {
        let dir = std::env::temp_dir().join(format!("deck-fsperm-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        create_dir(&dir, PRIVATE_DIR).expect("dir");
        let meta = std::fs::metadata(&dir).expect("meta");
        assert_eq!(mode(&meta), Some(0o700));
        let path = dir.join("f");
        let f = create_mode(OpenOptions::new().write(true).create(true), PRIVATE_FILE)
            .open(&path)
            .expect("open");
        set_mode(&f, PRIVATE_FILE).expect("chmod");
        let meta = std::fs::metadata(&path).expect("meta");
        assert_eq!(mode(&meta), Some(0o600));
        assert!(!is_executable(&meta));
        std::fs::remove_dir_all(&dir).expect("cleanup");
    }
}

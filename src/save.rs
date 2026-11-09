//! Crash-safe file replacement for saving documents.
//!
//! A save never truncates the destination directly. The bytes are first written
//! to a temporary file in the same directory, synchronized, and then renamed
//! over the destination. If anything fails before the rename, the original file
//! is left untouched.
//!
//! Behavior that is deliberately defined here:
//! - Symbolic links are followed: saving through a symlink updates its target
//!   and keeps the link in place, just like a normal `File::create` would.
//! - An existing destination keeps its permission bits on Unix. A new file is
//!   created with the usual umask-derived permissions. Because replacement is
//!   done by renaming inside the directory, the directory must be writable; a
//!   read-only file in a writable directory can still be replaced, and its mode
//!   is carried over to the new file.
//! - Replacing the destination breaks hard links to the old inode. That is an
//!   inherent consequence of atomic replacement.

use std::fs::{self, File, OpenOptions};
use std::io::{self, ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// Maximum number of symlinks followed while resolving the destination.
const MAX_SYMLINK_DEPTH: usize = 40;

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Write `bytes` to `path` without truncating the previous file until the new
/// contents are complete and safely stored in a temporary file.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let target = resolve_target(path)?;
    let parent = parent_dir(&target);
    let (temp_path, file) = create_temp(parent)?;

    let result = write_and_replace(&temp_path, file, &target, parent, bytes);
    if result.is_err() {
        let _ = fs::remove_file(&temp_path);
    }
    result
}

fn write_and_replace(
    temp_path: &Path,
    mut file: File,
    target: &Path,
    parent: &Path,
    bytes: &[u8],
) -> io::Result<()> {
    file.write_all(bytes)?;
    file.flush()?;

    #[cfg(unix)]
    match fs::metadata(target) {
        Ok(metadata) if !metadata.is_dir() => {
            fs::set_permissions(temp_path, metadata.permissions())?;
        }
        Ok(_) => {}
        Err(err) if err.kind() == ErrorKind::NotFound => {}
        Err(err) => return Err(err),
    }

    file.sync_all()?;
    drop(file);
    fs::rename(temp_path, target)?;
    sync_dir(parent);
    Ok(())
}

/// Follow a chain of symbolic links so that saving through a link updates its
/// target instead of replacing the link with a regular file. A dangling link is
/// resolved to its (not yet existing) target, matching the old behavior of
/// `std::fs::write`.
fn resolve_target(path: &Path) -> io::Result<PathBuf> {
    let mut current = path.to_path_buf();
    for _ in 0..MAX_SYMLINK_DEPTH {
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                let link = fs::read_link(&current)?;
                current = if link.is_absolute() {
                    link
                } else {
                    parent_dir(&current).join(link)
                };
            }
            Ok(_) => return Ok(current),
            Err(err) if err.kind() == ErrorKind::NotFound => return Ok(current),
            Err(err) => return Err(err),
        }
    }
    Err(io::Error::new(
        ErrorKind::InvalidInput,
        "too many levels of symbolic links",
    ))
}

fn parent_dir(path: &Path) -> &Path {
    match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    }
}

fn create_temp(parent: &Path) -> io::Result<(PathBuf, File)> {
    for _ in 0..100 {
        let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let file_name = format!(".mrs-save-{}-{counter}.tmp", std::process::id());
        let path = parent.join(file_name);
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => return Ok((path, file)),
            Err(err) if err.kind() == ErrorKind::AlreadyExists => continue,
            Err(err) => return Err(err),
        }
    }
    Err(io::Error::new(
        ErrorKind::AlreadyExists,
        "could not create a temporary save file",
    ))
}

/// Best-effort directory synchronization so the rename survives a crash.
#[cfg(unix)]
fn sync_dir(dir: &Path) {
    if let Ok(file) = File::open(dir) {
        let _ = file.sync_all();
    }
}

#[cfg(not(unix))]
fn sync_dir(_dir: &Path) {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU32;

    static TEST_DIR: AtomicU32 = AtomicU32::new(0);

    struct TestDir(PathBuf);

    impl TestDir {
        fn new() -> Self {
            loop {
                let n = TEST_DIR.fetch_add(1, Ordering::Relaxed);
                let path =
                    std::env::temp_dir().join(format!("mrs-save-test-{}-{n}", std::process::id()));
                match fs::create_dir(&path) {
                    Ok(()) => return Self(path),
                    Err(err) if err.kind() == ErrorKind::AlreadyExists => continue,
                    Err(err) => panic!("cannot create test directory: {err}"),
                }
            }
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn writes_new_file() {
        let dir = TestDir::new();
        let target = dir.path().join("new.txt");

        write_atomic(&target, "hello é\n".as_bytes()).unwrap();

        assert_eq!(fs::read(&target).unwrap(), "hello é\n".as_bytes());
    }

    #[test]
    fn replaces_existing_file() {
        let dir = TestDir::new();
        let target = dir.path().join("doc.txt");
        fs::write(&target, b"old contents").unwrap();

        write_atomic(&target, b"new contents").unwrap();

        assert_eq!(fs::read(&target).unwrap(), b"new contents");
    }

    #[test]
    fn rename_failure_cleans_up_temp_file() {
        let dir = TestDir::new();

        // A directory where the file is expected makes the final rename fail.
        // The temporary file must be cleaned up and the directory left alone.
        let obstructed = dir.path().join("obstructed");
        fs::create_dir(&obstructed).unwrap();
        fs::write(obstructed.join("keep"), b"old").unwrap();

        let result = write_atomic(&obstructed, b"new contents");

        assert!(result.is_err());
        assert!(obstructed.is_dir());
        assert_eq!(fs::read(obstructed.join("keep")).unwrap(), b"old");
        assert!(fs::read_dir(dir.path()).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .contains(".mrs-save-")
        }));
    }

    #[cfg(unix)]
    #[test]
    fn failed_save_leaves_existing_file_intact() {
        use std::os::unix::fs::PermissionsExt;

        let dir = TestDir::new();
        let target = dir.path().join("doc.txt");
        fs::write(&target, b"old contents").unwrap();

        let readonly = fs::Permissions::from_mode(0o555);
        fs::set_permissions(dir.path(), readonly).unwrap();

        // Running as root ignores directory permissions, so this test cannot
        // create the failure it checks for.
        if OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(dir.path().join("probe"))
            .is_ok()
        {
            fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o755)).unwrap();
            let _ = fs::remove_file(dir.path().join("probe"));
            return;
        }

        let result = write_atomic(&target, b"new contents");

        fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o755)).unwrap();

        assert!(result.is_err());
        assert_eq!(fs::read(&target).unwrap(), b"old contents");
    }

    #[cfg(unix)]
    #[test]
    fn preserves_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let dir = TestDir::new();
        let target = dir.path().join("doc.txt");
        fs::write(&target, b"old").unwrap();
        let mut permissions = fs::metadata(&target).unwrap().permissions();
        permissions.set_mode(0o640);
        fs::set_permissions(&target, permissions).unwrap();

        write_atomic(&target, b"new").unwrap();

        assert_eq!(
            fs::metadata(&target).unwrap().permissions().mode() & 0o777,
            0o640
        );
    }

    #[cfg(unix)]
    #[test]
    fn writes_through_symlink_without_replacing_it() {
        let dir = TestDir::new();
        let real = dir.path().join("real.txt");
        let link = dir.path().join("link.txt");
        fs::write(&real, b"old").unwrap();
        std::os::unix::fs::symlink(&real, &link).unwrap();

        write_atomic(&link, b"new").unwrap();

        assert!(
            fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(fs::read(&real).unwrap(), b"new");
    }
}

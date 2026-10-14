//! Publish a new empty private database only AFTER its ordinary FD is closed.
//! Existing SQLite inodes are never opened, truncated, replaced or chmodded.
use std::{
    ffi::CString,
    fs, io,
    os::unix::{ffi::OsStrExt, fs::PermissionsExt},
    path::Path,
};

pub(super) fn create(path: &Path) -> io::Result<()> {
    create_after_close(path, |_| {})
}

fn create_after_close(path: &Path, after_close: impl FnOnce(&Path)) -> io::Result<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let temporary = tempfile::Builder::new()
        .prefix(".ghtui-new-cache-")
        .permissions(fs::Permissions::from_mode(0o600))
        .tempfile_in(parent)?
        .into_temp_path(); // Closes File BEFORE any database name is published.
    after_close(&temporary);
    let from = CString::new(temporary.as_os_str().as_bytes())?;
    let to = CString::new(path.as_os_str().as_bytes())?;
    // SAFETY: both paths are NUL-terminated, and no descriptors for the
    // published inode are opened or closed here. Never allow overwrite or a
    // hardlink fallback; unsupported filesystems fail closed.
    let result = unsafe { rename_exclusive(&from, &to) };
    if result != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(target_os = "linux")]
unsafe fn rename_exclusive(from: &CString, to: &CString) -> libc::c_int {
    // Use the kernel operation directly instead of imposing a newer glibc
    // renameat2 wrapper symbol on supported Linux binaries.
    // SAFETY: forwarded pointer/lifetime contract from the caller; syscall
    // arguments exactly match renameat2 and only its zero/-1 result is used.
    unsafe {
        libc::syscall(
            libc::SYS_renameat2,
            libc::AT_FDCWD,
            from.as_ptr(),
            libc::AT_FDCWD,
            to.as_ptr(),
            libc::RENAME_NOREPLACE,
        ) as libc::c_int
    }
}

#[cfg(target_os = "macos")]
unsafe fn rename_exclusive(from: &CString, to: &CString) -> libc::c_int {
    // SAFETY: forwarded pointer/lifetime contract from the caller. Apple's
    // exclusive rename is available since macOS10.12, unlike regular mknod.
    unsafe {
        libc::renameatx_np(
            libc::AT_FDCWD,
            from.as_ptr(),
            libc::AT_FDCWD,
            to.as_ptr(),
            libc::RENAME_EXCL,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::MetadataExt;

    fn directory() -> tempfile::TempDir {
        tempfile::Builder::new()
            .permissions(fs::Permissions::from_mode(0o700))
            .tempdir()
            .unwrap()
    }

    #[test]
    fn cache_storage_closed_creator_publishes_exact_private_single_link_without_leftovers() {
        let directory = directory();
        let path = directory.path().join("cache.sqlite");
        let mut original = None;
        create_after_close(&path, |temporary| {
            assert!(
                !path.exists(),
                "database name must still be unpublished after FD close"
            );
            let metadata = fs::symlink_metadata(temporary).unwrap();
            assert!(metadata.is_file());
            assert_eq!(
                (metadata.mode() & 0o777, metadata.nlink(), metadata.len()),
                (0o600, 1, 0)
            );
            original = Some((metadata.dev(), metadata.ino()));
            #[cfg(target_os = "linux")]
            for entry in fs::read_dir("/proc/self/fd").unwrap() {
                let entry = entry.unwrap();
                if let Ok(fd) = fs::metadata(entry.path()) {
                    assert_ne!(
                        (fd.dev(), fd.ino()),
                        original.unwrap(),
                        "ordinary file FD must actually be CLOSED before publication"
                    );
                }
            }
        })
        .unwrap();
        let metadata = fs::symlink_metadata(&path).unwrap();
        assert_eq!((metadata.dev(), metadata.ino()), original.unwrap());
        assert_eq!(
            (metadata.mode() & 0o777, metadata.nlink(), metadata.len()),
            (0o600, 1, 0)
        );
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
    }

    #[test]
    fn cache_storage_closed_creator_refuses_existing_regular_symlink_and_hardlink_unchanged() {
        for kind in ["regular", "symlink", "hardlink"] {
            let directory = directory();
            let path = directory.path().join("cache.sqlite");
            let other = directory.path().join("unrelated");
            fs::write(&other, b"original unrelated bytes").unwrap();
            fs::set_permissions(&other, fs::Permissions::from_mode(0o644)).unwrap();
            match kind {
                "regular" => fs::write(&path, b"original database bytes").unwrap(),
                "symlink" => std::os::unix::fs::symlink(&other, &path).unwrap(),
                _ => fs::hard_link(&other, &path).unwrap(),
            }
            let before = fs::symlink_metadata(&path).unwrap();
            assert_eq!(
                create(&path).unwrap_err().kind(),
                io::ErrorKind::AlreadyExists
            );
            let after = fs::symlink_metadata(&path).unwrap();
            assert_eq!(
                (before.dev(), before.ino(), before.mode(), before.nlink()),
                (after.dev(), after.ino(), after.mode(), after.nlink())
            );
            assert_eq!(fs::read(&other).unwrap(), b"original unrelated bytes");
            assert_eq!(fs::metadata(&other).unwrap().mode() & 0o777, 0o644);
            if kind == "regular" {
                assert_eq!(fs::read(&path).unwrap(), b"original database bytes");
            }
            assert_eq!(
                fs::read_dir(directory.path()).unwrap().count(),
                2,
                "failed creation must clean only its own unpublished temporary path"
            );
        }
    }

    #[test]
    fn cache_storage_closed_creator_same_directory_race_never_clobbers_new_alias() {
        let directory = directory();
        let path = directory.path().join("cache.sqlite");
        let other = directory.path().join("unrelated");
        fs::write(&other, b"unaltered despite last-moment target substitution").unwrap();
        let error = create_after_close(&path, |_| {
            std::os::unix::fs::symlink(&other, &path).unwrap();
        })
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
        assert!(fs::symlink_metadata(&path).unwrap().is_symlink());
        assert_eq!(
            fs::read(&other).unwrap(),
            b"unaltered despite last-moment target substitution"
        );
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 2);
    }
}

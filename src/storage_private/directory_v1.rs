//! macOS private preparation without opening/closing any SQLite file FD.
//! Only the owned directory is held; fstatat/fchmodat use no-follow lookups.
//! Also compiled in Linux unit tests to exercise the SAME POSIX state policy;
//! that is not native Apple runtime evidence.
use anyhow::{bail, Context, Result};
use std::{
    ffi::CString,
    fs::{self, File},
    io,
    mem::MaybeUninit,
    os::{
        fd::AsRawFd,
        unix::{
            ffi::OsStrExt,
            fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
        },
    },
    path::Path,
};

pub(super) fn prepare(path: &Path) -> Result<Vec<File>> {
    prepare_after_snapshot(path, |_| {})
}

fn stat_at(directory: &File, name: &CString) -> io::Result<Option<libc::stat>> {
    let mut metadata = MaybeUninit::uninit();
    // SAFETY: name is terminated, directory remains open, output has stat size.
    let result = unsafe {
        libc::fstatat(
            directory.as_raw_fd(),
            name.as_ptr(),
            metadata.as_mut_ptr(),
            libc::AT_SYMLINK_NOFOLLOW,
        )
    };
    if result == 0 {
        // SAFETY: successful fstatat initialized the complete stat record.
        Ok(Some(unsafe { metadata.assume_init() }))
    } else {
        let error = io::Error::last_os_error();
        if error.kind() == io::ErrorKind::NotFound {
            Ok(None)
        } else {
            Err(error)
        }
    }
}
fn same(left: &libc::stat, right: &libc::stat) -> bool {
    left.st_dev == right.st_dev && left.st_ino == right.st_ino
}
fn verify(metadata: &libc::stat, uid: libc::uid_t) -> Result<()> {
    if metadata.st_mode & libc::S_IFMT != libc::S_IFREG
        || metadata.st_uid != uid
        || metadata.st_nlink != 1
    {
        bail!("Cache database/journal target must be an owned regular file, not a symlink/hardlink/device");
    }
    Ok(())
}
fn snapshot(
    directory: &File,
    names: &[CString],
    uid: libc::uid_t,
) -> Result<Vec<Option<libc::stat>>> {
    names
        .iter()
        .map(|name| {
            let value = stat_at(directory, name)?;
            if let Some(metadata) = &value {
                verify(metadata, uid)?;
            }
            Ok(value)
        })
        .collect()
}
fn parent_current(parent: &Path, directory: &File, uid: libc::uid_t) -> Result<()> {
    let named = fs::symlink_metadata(parent)?;
    if !named.is_dir()
        || named.uid() != uid
        || named.mode() & 0o022 != 0
        || !super::same_file(&named, &directory.metadata()?)
    {
        bail!("Original owned cache directory changed; no SQLite opened");
    }
    Ok(())
}

fn prepare_after_snapshot(path: &Path, mut observe: impl FnMut(usize)) -> Result<Vec<File>> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(parent)?;
    let original_directory = fs::symlink_metadata(parent)?;
    // SAFETY: geteuid has no pointers or side effects.
    let uid = unsafe { libc::geteuid() };
    if !original_directory.is_dir()
        || original_directory.uid() != uid
        || original_directory.mode() & 0o022 != 0
    {
        bail!("Cache parent must be an owned directory without group/other write access; no private data opened");
    }
    let directory = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(parent)?;
    if !super::same_file(&original_directory, &directory.metadata()?) {
        bail!("Original owned cache directory changed while opening; no SQLite opened");
    }
    let paths = super::target_paths(path);
    let names = paths
        .iter()
        .map(|path| {
            CString::new(
                path.file_name()
                    .context("Cache path requires a literal filename")?
                    .as_bytes(),
            )
            .map_err(anyhow::Error::from)
        })
        .collect::<Result<Vec<_>>>()?;
    // All existing names are checked BEFORE creating an absent database too.
    let mut original = snapshot(&directory, &names, uid)?;
    if original[0].is_none() {
        parent_current(parent, &directory, uid)?;
        super::create_v1::create(path)?;
        original = snapshot(&directory, &names, uid)?;
    }
    let main =
        original[0].context("Cache database disappeared after creation; no replacement created")?;
    for attempt in 0..4 {
        if attempt != 0 {
            original = snapshot(&directory, &names, uid)?;
        }
        if original[0]
            .as_ref()
            .is_none_or(|current| !same(&main, current))
        {
            bail!("Original cache database changed before repair; no modes repaired");
        }
        observe(attempt);
        parent_current(parent, &directory, uid)?;
        let current = snapshot(&directory, &names, uid)?;
        if current[0]
            .as_ref()
            .is_none_or(|current| !same(&main, current))
        {
            bail!("Original cache database changed before repair; no modes repaired");
        }
        if original
            .iter()
            .zip(&current)
            .any(|(old, now)| match (old, now) {
                (None, None) => false,
                (Some(old), Some(now)) => !same(old, now),
                _ => true,
            })
        {
            continue;
        }
        // No database/journal descriptors are opened or closed here. Recheck
        // the owned original name immediately before each no-follow chmod.
        for (index, (name, expected)) in names.iter().zip(&current).enumerate() {
            let Some(expected) = expected else {
                continue;
            };
            let Some(now) = stat_at(&directory, name)? else {
                if index != 0 {
                    continue;
                }
                bail!("Original cache database disappeared during repair; no SQLite opened");
            };
            verify(&now, uid)?;
            if !same(expected, &now) {
                bail!("Cache target changed during private repair; no SQLite opened");
            }
            // SAFETY: live directory FD, terminated single-component name,
            // no-follow flag. Apple's implementation performs a vnode chmod,
            // not an ordinary open/close that could cancel SQLite locks.
            let result = unsafe {
                libc::fchmodat(
                    directory.as_raw_fd(),
                    name.as_ptr(),
                    0o600,
                    libc::AT_SYMLINK_NOFOLLOW,
                )
            };
            if result != 0 {
                let error = io::Error::last_os_error();
                if index != 0 && error.kind() == io::ErrorKind::NotFound {
                    continue;
                }
                return Err(error.into());
            }
        }
        parent_current(parent, &directory, uid)?;
        let repaired = snapshot(&directory, &names, uid)?;
        if repaired[0]
            .as_ref()
            .is_none_or(|current| !same(&main, current))
        {
            bail!("Original cache database changed during repair; no SQLite opened");
        }
        if repaired
            .iter()
            .flatten()
            .any(|metadata| metadata.st_mode & 0o777 != 0o600)
        {
            bail!("Cache journals changed during private preparation; no SQLite opened");
        }
        return Ok(vec![directory]);
    }
    bail!("Cache journals kept changing during bounded validation; no modes repaired")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    fn directory() -> tempfile::TempDir {
        tempfile::Builder::new()
            .permissions(fs::Permissions::from_mode(0o700))
            .tempdir()
            .unwrap()
    }
    fn legacy(path: &Path, bytes: &[u8]) {
        fs::write(path, bytes).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o644)).unwrap();
    }

    #[test]
    fn cache_storage_directory_metadata_repairs_all_original_nonempty_names_without_file_fds() {
        let directory = directory();
        let path = directory.path().join("cache.sqlite");
        let paths = super::super::target_paths(&path);
        for path in &paths {
            legacy(path, b"original nonempty bytes");
        }
        let held = prepare(&path).unwrap();
        assert_eq!(held.len(), 1);
        assert!(held[0].metadata().unwrap().is_dir());
        for path in &paths {
            assert_eq!(fs::read(path).unwrap(), b"original nonempty bytes");
            assert_eq!(fs::metadata(path).unwrap().mode() & 0o777, 0o600);
        }
    }

    #[test]
    fn cache_storage_directory_metadata_rejects_aliases_before_any_mode_repair_or_database_creation(
    ) {
        for main_exists in [false, true] {
            for hardlink in [false, true] {
                let directory = directory();
                let path = directory.path().join("cache.sqlite");
                let wal = directory.path().join("cache.sqlite-wal");
                let unrelated = directory.path().join("unrelated");
                legacy(&unrelated, b"unrelated original bytes");
                if main_exists {
                    legacy(&path, b"original database");
                }
                if hardlink {
                    fs::hard_link(&unrelated, &wal).unwrap();
                } else {
                    std::os::unix::fs::symlink(&unrelated, &wal).unwrap();
                }
                assert!(prepare(&path).is_err());
                assert_eq!(fs::read(&unrelated).unwrap(), b"unrelated original bytes");
                assert_eq!(fs::metadata(&unrelated).unwrap().mode() & 0o777, 0o644);
                assert_eq!(path.exists(), main_exists);
                if main_exists {
                    assert_eq!(fs::read(&path).unwrap(), b"original database");
                    assert_eq!(fs::metadata(&path).unwrap().mode() & 0o777, 0o644);
                }
            }
        }
    }

    #[test]
    fn cache_storage_directory_metadata_actual_last_wal_close_revalidates_without_rejecting_safe_cleanup(
    ) {
        let directory = directory();
        let path = directory.path().join("cache.sqlite");
        let wal = directory.path().join("cache.sqlite-wal");
        let connection = rusqlite::Connection::open(&path).unwrap();
        connection.execute_batch("PRAGMA journal_mode=WAL; CREATE TABLE evidence(value TEXT); INSERT INTO evidence VALUES('genuinely committed original bytes');").unwrap();
        assert!(wal.is_file());
        let mut last = Some(connection);
        let mut calls = 0;
        let held = prepare_after_snapshot(&path, |_| {
            calls += 1;
            if let Some(connection) = last.take() {
                drop(connection);
                assert!(!wal.exists(), "actual SQLite last close must clean its WAL");
            }
        })
        .unwrap();
        assert_eq!(
            calls, 2,
            "one bounded fresh validation, not a skipped guard"
        );
        assert_eq!(held.len(), 1);
        let connection = rusqlite::Connection::open(&path).unwrap();
        let bytes: String = connection
            .query_row("SELECT value FROM evidence", [], |row| row.get(0))
            .unwrap();
        assert_eq!(bytes, "genuinely committed original bytes");
    }

    #[test]
    fn cache_storage_directory_metadata_late_alias_and_main_removal_never_repair_or_recreate() {
        for removed_main in [false, true] {
            let directory = directory();
            let path = directory.path().join("cache.sqlite");
            let wal = directory.path().join("cache.sqlite-wal");
            let other = directory.path().join("unrelated");
            legacy(&path, b"original database");
            legacy(&other, b"unrelated private bytes");
            let result = prepare_after_snapshot(&path, |_| {
                if removed_main {
                    fs::remove_file(&path).unwrap();
                } else {
                    std::os::unix::fs::symlink(&other, &wal).unwrap();
                }
            });
            assert!(result.is_err());
            assert_eq!(path.exists(), !removed_main);
            if !removed_main {
                assert_eq!(fs::metadata(&path).unwrap().mode() & 0o777, 0o644);
            }
            assert_eq!(fs::metadata(&other).unwrap().mode() & 0o777, 0o644);
            assert_eq!(fs::read(&other).unwrap(), b"unrelated private bytes");
        }
    }

    #[test]
    fn cache_storage_directory_metadata_persistent_optional_churn_is_bounded_without_repairs() {
        let directory = directory();
        let path = directory.path().join("cache.sqlite");
        let wal = directory.path().join("cache.sqlite-wal");
        legacy(&path, b"original database");
        let mut calls = 0;
        let result = prepare_after_snapshot(&path, |_| {
            calls += 1;
            if wal.exists() {
                fs::remove_file(&wal).unwrap();
            } else {
                legacy(&wal, b"new actual optional journal");
            }
        });
        assert_eq!(calls, 4);
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("bounded validation"));
        assert_eq!(fs::metadata(&path).unwrap().mode() & 0o777, 0o644);
    }

    #[test]
    fn cache_storage_directory_metadata_preserves_actual_cross_process_wal_writer_lock() {
        const CHILD: &str = "GHTUI_CACHE_DIRECTORY_LOCK_PROBE_V1";
        const MODE: &str = "GHTUI_CACHE_DIRECTORY_LOCK_MODE_V1";
        if let Some(path) = std::env::var_os(CHILD) {
            let connection = rusqlite::Connection::open(Path::new(&path)).unwrap();
            connection.busy_timeout(std::time::Duration::ZERO).unwrap();
            let value: String = connection
                .query_row("SELECT value FROM lock_evidence", [], |row| row.get(0))
                .unwrap();
            if std::env::var(MODE).unwrap() == "busy" {
                assert_eq!(value, "original committed bytes");
                let error = connection.execute_batch("BEGIN IMMEDIATE").unwrap_err();
                assert_eq!(
                    error.sqlite_error_code(),
                    Some(rusqlite::ErrorCode::DatabaseBusy)
                );
                println!("ACTUAL_FRESH_SQLITE_WRITER_BUSY");
            } else {
                assert_eq!(value, "parent committed latest");
                connection
                    .execute_batch("BEGIN IMMEDIATE; ROLLBACK;")
                    .unwrap();
                println!("ACTUAL_FRESH_SQLITE_COMMIT_VISIBLE");
            }
            return;
        }
        let directory = directory();
        let path = directory.path().join("cache.sqlite");
        let connection = rusqlite::Connection::open(&path).unwrap();
        connection.execute_batch("PRAGMA journal_mode=WAL; CREATE TABLE lock_evidence(value TEXT); INSERT INTO lock_evidence VALUES('original committed bytes'); BEGIN IMMEDIATE; UPDATE lock_evidence SET value='parent committed latest';").unwrap();
        let probe = |mode: &str| {
            use crate::providers::runner::{CommandRunner, CommandSpec};
            let spec = CommandSpec::new(std::env::current_exe().unwrap().into_os_string())
                .args(["--exact", "storage_private::directory_v1::tests::cache_storage_directory_metadata_preserves_actual_cross_process_wal_writer_lock", "--nocapture", "--test-threads=1"])
                .env(CHILD, path.as_os_str()).env(MODE, mode).timeout(std::time::Duration::from_secs(5)).fresh_read();
            let output = CommandRunner::default()
                .run(spec, &Default::default())
                .unwrap();
            assert_eq!(output.exit_code, 0);
            let stdout = String::from_utf8(output.stdout).unwrap();
            assert!(
                stdout.contains("1 passed; 0 failed; 0 ignored"),
                "actual child selected/executed count: {stdout}"
            );
            assert!(stdout.contains(if mode == "busy" {
                "ACTUAL_FRESH_SQLITE_WRITER_BUSY"
            } else {
                "ACTUAL_FRESH_SQLITE_COMMIT_VISIBLE"
            }));
        };
        probe("busy");
        drop(prepare(&path).unwrap());
        probe("busy"); // No SQLite file FD close may silently release our lock.
        connection.execute_batch("COMMIT;").unwrap();
        probe("committed");
        let integrity: String = connection
            .query_row("PRAGMA integrity_check", [], |row| row.get(0))
            .unwrap();
        assert_eq!(integrity, "ok");
    }
}

//! Unix-only private SQLite preparation. Never follows or removes aliases.
#[cfg(target_os = "linux")]
use anyhow::bail;
use anyhow::Result;
use std::{
    fs::{self, File},
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
};
mod create_v1;
#[cfg(any(target_os = "macos", test))]
mod directory_v1;
#[cfg(target_os = "linux")]
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};

/// Hold validated metadata handles until SQLite has opened the database. New
/// journals inherit0600; existing nonempty legacy journals need explicit repair.
/// Linux O_PATH handles do not cancel SQLite's POSIX locks on close.
#[cfg(target_os = "linux")]
pub(crate) fn prepare(path: &Path) -> Result<Vec<File>> {
    prepare_after_open(path, |_, _| {})
}

#[cfg(target_os = "macos")]
pub(crate) fn prepare(path: &Path) -> Result<Vec<File>> {
    directory_v1::prepare(path)
}

fn target_paths(path: &Path) -> [PathBuf; 4] {
    let append = |suffix: &str| {
        let mut name = path.as_os_str().to_os_string();
        name.push(suffix);
        PathBuf::from(name)
    };
    [
        path.to_path_buf(),
        append("-wal"),
        append("-shm"),
        append("-journal"),
    ]
}

#[cfg(target_os = "linux")]
fn prepare_after_open(path: &Path, mut after_open: impl FnMut(&Path, &File)) -> Result<Vec<File>> {
    prepare_with_named_metadata_v2(path, &mut after_open, |path, _| fs::symlink_metadata(path))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NamedPhaseV2 {
    Initial,
    Current,
    #[cfg(any(target_os = "macos", test))]
    BeforeRepair,
    #[cfg(any(target_os = "macos", test))]
    Repaired,
}

/// An unlinked optional name is a transient observation, NEVER an admissible
/// target. Every other type/owner/link combination still passes strict verify.
fn optional_named_unlinked_v2(index: usize, regular: bool, owned: bool, unlinked: bool) -> bool {
    index > 0 && regular && owned && unlinked
}

#[cfg(target_os = "linux")]
fn prepare_with_named_metadata_v2(
    path: &Path,
    mut after_open: impl FnMut(&Path, &File),
    mut metadata: impl FnMut(&Path, NamedPhaseV2) -> std::io::Result<fs::Metadata>,
) -> Result<Vec<File>> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(parent)?;
    let directory = fs::symlink_metadata(parent)?;
    // SAFETY: geteuid takes no pointers and has no side effects on both supported
    // Unix platforms. This checks OS ownership, not GitHub authentication.
    let uid = unsafe { libc::geteuid() };
    if !directory.is_dir() || directory.uid() != uid || directory.mode() & 0o022 != 0 {
        bail!("Cache parent must be an owned directory without group/other write access; no private data opened");
    }
    // Validate every existing target before repairing any mode. Refusing an
    // unsafe journal must not alter an unrelated alias target or the database.
    let paths = target_paths(path);
    let mut original_main = None;
    // SQLite may unlink optional journals during an ordinary commit/last close.
    // Never repair a detached descriptor or relax the current-target policy.
    // Discard the WHOLE tentative set and inspect fresh names, at most four
    // times, before any chmod. This performs no SQL or mutation retry.
    'validation: for _ in 0..4 {
        // Validate ALL existing names before even creating an absent main.
        // An unsafe optional alias must not cause any database publication.
        let mut originals = Vec::with_capacity(paths.len());
        let mut transient = false;
        for (index, path) in paths.iter().enumerate() {
            let original = match metadata(path, NamedPhaseV2::Initial) {
                Ok(metadata) => {
                    if optional_named_unlinked_v2(
                        index,
                        metadata.is_file(),
                        metadata.uid() == uid,
                        metadata.nlink() == 0,
                    ) {
                        transient = true;
                    } else {
                        verify(&metadata, uid)?;
                        if index == 0 {
                            if original_main
                                .as_ref()
                                .is_some_and(|original| !same_file(original, &metadata))
                            {
                                bail!("Original cache database changed during validation; no modes repaired");
                            }
                            original_main.get_or_insert_with(|| metadata.clone());
                        }
                    }
                    Some(metadata)
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound && index > 0 => None,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    if original_main.is_some() {
                        bail!("Original cache database disappeared during validation; no replacement created");
                    }
                    None
                }
                Err(error) => return Err(error.into()),
            };
            originals.push(original);
        }
        if transient {
            continue;
        }
        let mut descriptors = Vec::new();
        for (index, path) in paths.iter().enumerate() {
            let original = &originals[index];
            if index > 0 && original.is_none() {
                continue;
            }
            let file = match open_target(path, index == 0 && original.is_none()) {
                Ok(file) => file,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound && index > 0 => {
                    continue 'validation;
                }
                Err(error) => return Err(error.into()),
            };
            after_open(path, &file);
            let opened = file.metadata()?;
            if index > 0
                && opened.is_file()
                && opened.uid() == uid
                && opened.nlink() == 0
                && original.as_ref().is_some_and(|old| same_file(old, &opened))
            {
                continue 'validation;
            }
            verify(&opened, uid)?;
            if index == 0 {
                if original_main
                    .as_ref()
                    .is_some_and(|original| !same_file(original, &opened))
                {
                    bail!("Original cache database changed while opening; no modes repaired");
                }
                original_main.get_or_insert_with(|| opened.clone());
            }
            if original
                .as_ref()
                .is_some_and(|old| !same_file(old, &opened))
            {
                if index > 0 {
                    continue 'validation;
                }
                bail!("Original cache database changed while opening; no modes repaired");
            }
            descriptors.push((index, path, file));
        }
        // Validate every held descriptor AND current name before changing any
        // mode. A newly substituted alias is an error, not a transient success.
        for (index, path) in paths.iter().enumerate() {
            let file = descriptors
                .iter()
                .find(|(held_index, _, _)| *held_index == index)
                .map(|(_, _, file)| file);
            let current = match metadata(path, NamedPhaseV2::Current) {
                Ok(metadata) => metadata,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound && index > 0 => {
                    if file.is_some() {
                        continue 'validation;
                    }
                    continue;
                }
                Err(error) => return Err(error.into()),
            };
            if optional_named_unlinked_v2(
                index,
                current.is_file(),
                current.uid() == uid,
                current.nlink() == 0,
            ) {
                continue 'validation;
            }
            verify(&current, uid)?;
            let Some(file) = file else {
                // A formerly absent journal appeared. It needs its own freshly
                // no-follow descriptor; an alias was already rejected above.
                continue 'validation;
            };
            let held = file.metadata()?;
            if index > 0 && held.is_file() && held.uid() == uid && held.nlink() == 0 {
                continue 'validation;
            }
            verify(&held, uid)?;
            if !same_file(&held, &current) {
                if index > 0 {
                    continue 'validation;
                }
                bail!("Original cache database changed before repair; no modes repaired");
            }
        }
        for (_, _, file) in &descriptors {
            repair_mode(file)?;
        }
        return Ok(descriptors.into_iter().map(|(_, _, file)| file).collect());
    }
    bail!("Cache journals kept changing during bounded validation; no modes repaired")
}

#[cfg(target_os = "linux")]
fn open_target(path: &Path, create: bool) -> std::io::Result<File> {
    if create {
        create_v1::create(path)?;
    }
    fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_PATH | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)
}

#[cfg(target_os = "linux")]
fn repair_mode(file: &File) -> std::io::Result<()> {
    use std::os::fd::AsRawFd;
    // This is chmod on our pinned regular inode, not a re-open of the database
    // or a lookup through its mutable original name. O_PATH close preserves
    // POSIX locks (Linux filp_flush skips locks_remove_posix for FMODE_PATH).
    // The same technique is used by glibc's fchmodat no-follow fallback. If
    // procfs is unavailable, fail closed; never fall back to an ordinary FD.
    fs::set_permissions(
        format!("/proc/self/fd/{}", file.as_raw_fd()),
        fs::Permissions::from_mode(0o600),
    )
}

fn same_file(left: &fs::Metadata, right: &fs::Metadata) -> bool {
    left.dev() == right.dev() && left.ino() == right.ino()
}

#[cfg(target_os = "linux")]
fn verify(metadata: &fs::Metadata, uid: u32) -> Result<()> {
    if !metadata.is_file() || metadata.uid() != uid || metadata.nlink() != 1 {
        bail!("Cache database/journal target must be an owned regular file, not a symlink/hardlink/device");
    }
    Ok(())
}

#[cfg(test)]
mod named_race_v2_tests;

#[cfg(all(test, target_os = "linux"))]
mod churn_trace_v4_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[cfg(target_os = "linux")]
    #[test]
    fn cache_storage_private_preparation_preserves_actual_live_sqlite_posix_locks() {
        let directory = tempfile::Builder::new()
            .permissions(fs::Permissions::from_mode(0o700))
            .tempdir()
            .unwrap();
        let path = directory.path().join("cache.sqlite");
        let shm = directory.path().join("cache.sqlite-shm");
        let connection = rusqlite::Connection::open(&path).unwrap();
        connection.execute_batch("PRAGMA journal_mode=WAL; CREATE TABLE lock_evidence(value TEXT); INSERT INTO lock_evidence VALUES('committed original');").unwrap();
        let database_inode = fs::metadata(&path).unwrap().ino().to_string();
        let shm_inode = fs::metadata(&shm).unwrap().ino().to_string();
        let locks = || {
            fs::read_to_string("/proc/locks")
                .unwrap()
                .lines()
                .filter_map(|line| {
                    let fields: Vec<_> = line.split_whitespace().collect();
                    (fields.len() >= 8
                        && fields[1] == "POSIX"
                        && fields[4] == std::process::id().to_string()
                        && fields[5]
                            .rsplit(':')
                            .next()
                            .is_some_and(|inode| inode == database_inode || inode == shm_inode))
                    .then(|| fields[1..].join(" "))
                })
                .collect::<std::collections::BTreeSet<_>>()
        };
        let before = locks();
        assert!(
            before.len() >= 2,
            "real SQLite must hold database and WAL deadman-switch locks: {before:?}"
        );
        drop(prepare(&path).unwrap());
        assert_eq!(
            locks(),
            before,
            "private-file validation must not silently cancel live SQLite locks"
        );
        let actual: String = connection
            .query_row("SELECT value FROM lock_evidence", [], |row| row.get(0))
            .unwrap();
        assert_eq!(actual, "committed original");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn cache_storage_private_actual_sqlite_last_wal_close_after_open_is_not_an_unsafe_alias() {
        let directory = tempfile::Builder::new()
            .permissions(fs::Permissions::from_mode(0o700))
            .tempdir()
            .unwrap();
        let path = directory.path().join("cache.sqlite");
        let wal = directory.path().join("cache.sqlite-wal");
        let connection = rusqlite::Connection::open(&path).unwrap();
        connection
            .execute_batch("PRAGMA journal_mode=WAL; CREATE TABLE evidence(value TEXT); INSERT INTO evidence VALUES('original committed private bytes');")
            .unwrap();
        assert!(fs::metadata(&wal).unwrap().len() > 0);
        let mut last = Some(connection);
        let mut observed_detached = false;
        let descriptors = prepare_after_open(&path, |opened_path, file| {
            if opened_path == wal {
                let before = file.metadata().unwrap();
                assert_eq!(before.nlink(), 1);
                assert_eq!(before.ino(), fs::metadata(&wal).unwrap().ino());
                drop(last.take()); // Actual SQLite cleanup, not manual WAL deletion.
                let after = file.metadata().unwrap();
                assert_eq!(after.ino(), before.ino());
                assert_eq!(after.nlink(), 0);
                assert!(!wal.exists());
                observed_detached = true;
            }
        })
        .unwrap();
        assert!(observed_detached, "the exact after-open race did not occur");
        assert_eq!(descriptors.len(), 1);
        let read = rusqlite::Connection::open(&path).unwrap();
        let bytes: String = read
            .query_row("SELECT value FROM evidence", [], |row| row.get(0))
            .unwrap();
        assert_eq!(bytes, "original committed private bytes");
        assert_eq!(fs::metadata(&path).unwrap().mode() & 0o777, 0o600);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn cache_storage_private_detached_optional_journal_then_symlink_or_hardlink_refuses_before_any_repair(
    ) {
        for hardlink in [false, true] {
            let directory = tempfile::Builder::new()
                .permissions(fs::Permissions::from_mode(0o700))
                .tempdir()
                .unwrap();
            let path = directory.path().join("cache.sqlite");
            let journal = directory.path().join("cache.sqlite-journal");
            let unrelated = directory.path().join("unrelated");
            for (file, bytes) in [
                (&path, b"original database".as_slice()),
                (&journal, b"original journal"),
                (&unrelated, b"unrelated private bytes"),
            ] {
                fs::write(file, bytes).unwrap();
                fs::set_permissions(file, fs::Permissions::from_mode(0o644)).unwrap();
            }
            let mut substituted = false;
            let result = prepare_after_open(&path, |opened_path, held| {
                if opened_path == journal && !substituted {
                    fs::remove_file(&journal).unwrap();
                    assert_eq!(held.metadata().unwrap().nlink(), 0);
                    if hardlink {
                        fs::hard_link(&unrelated, &journal).unwrap();
                    } else {
                        std::os::unix::fs::symlink(&unrelated, &journal).unwrap();
                    }
                    substituted = true;
                }
            });
            assert!(substituted);
            assert!(result.is_err(), "a replacement alias was accepted");
            assert_eq!(fs::read(&path).unwrap(), b"original database");
            assert_eq!(fs::read(&unrelated).unwrap(), b"unrelated private bytes");
            for file in [&path, &unrelated] {
                assert_eq!(fs::metadata(file).unwrap().mode() & 0o777, 0o644);
            }
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn cache_storage_private_late_alias_in_previously_absent_optional_slot_refuses_before_repair() {
        let directory = tempfile::Builder::new()
            .permissions(fs::Permissions::from_mode(0o700))
            .tempdir()
            .unwrap();
        let path = directory.path().join("cache.sqlite");
        let wal = directory.path().join("cache.sqlite-wal");
        let journal = directory.path().join("cache.sqlite-journal");
        let unrelated = directory.path().join("unrelated");
        for file in [&path, &journal, &unrelated] {
            fs::write(file, b"untouched original bytes").unwrap();
            fs::set_permissions(file, fs::Permissions::from_mode(0o644)).unwrap();
        }
        let mut substituted = false;
        let result = prepare_after_open(&path, |opened_path, _| {
            if opened_path == journal {
                assert!(!wal.exists());
                std::os::unix::fs::symlink(&unrelated, &wal).unwrap();
                substituted = true;
            }
        });
        assert!(substituted);
        assert!(result.is_err());
        for file in [&path, &journal, &unrelated] {
            assert_eq!(fs::read(file).unwrap(), b"untouched original bytes");
            assert_eq!(fs::metadata(file).unwrap().mode() & 0o777, 0o644);
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn cache_storage_private_removed_main_database_never_recreates_or_repairs_optional_files() {
        let directory = tempfile::Builder::new()
            .permissions(fs::Permissions::from_mode(0o700))
            .tempdir()
            .unwrap();
        let path = directory.path().join("cache.sqlite");
        let journal = directory.path().join("cache.sqlite-journal");
        for file in [&path, &journal] {
            fs::write(file, b"preserve original").unwrap();
            fs::set_permissions(file, fs::Permissions::from_mode(0o644)).unwrap();
        }
        let result = prepare_after_open(&path, |opened_path, held| {
            if opened_path == path {
                fs::remove_file(&path).unwrap();
                assert_eq!(held.metadata().unwrap().nlink(), 0);
            }
        });
        assert!(result.is_err());
        assert!(
            !path.exists(),
            "the removed database was silently recreated"
        );
        assert_eq!(fs::read(&journal).unwrap(), b"preserve original");
        assert_eq!(fs::metadata(&journal).unwrap().mode() & 0o777, 0o644);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn cache_storage_private_repeated_optional_journal_churn_stops_at_four_passes_without_repair() {
        let directory = tempfile::Builder::new()
            .permissions(fs::Permissions::from_mode(0o700))
            .tempdir()
            .unwrap();
        let path = directory.path().join("cache.sqlite");
        let journal = directory.path().join("cache.sqlite-journal");
        for file in [&path, &journal] {
            fs::write(file, b"original private bytes").unwrap();
            fs::set_permissions(file, fs::Permissions::from_mode(0o644)).unwrap();
        }
        let mut passes = 0;
        let result = prepare_after_open(&path, |opened_path, held| {
            if opened_path == journal {
                fs::remove_file(&journal).unwrap();
                assert_eq!(held.metadata().unwrap().nlink(), 0);
                fs::write(&journal, b"original private bytes").unwrap();
                fs::set_permissions(&journal, fs::Permissions::from_mode(0o644)).unwrap();
                passes += 1;
            }
        });
        assert_eq!(passes, 4, "unbounded or silently accepted unstable target");
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("bounded validation"));
        for file in [&path, &journal] {
            assert_eq!(fs::read(file).unwrap(), b"original private bytes");
            assert_eq!(fs::metadata(file).unwrap().mode() & 0o777, 0o644);
        }
    }

    #[test]
    fn cache_storage_private_existing_nonempty_journals_are_repaired_without_truncation() {
        let directory = tempfile::Builder::new()
            .permissions(fs::Permissions::from_mode(0o700))
            .tempdir()
            .unwrap();
        let path = directory.path().join("cache.sqlite");
        let expected = b"existing bytes must stay intact";
        for name in [
            "cache.sqlite",
            "cache.sqlite-wal",
            "cache.sqlite-shm",
            "cache.sqlite-journal",
        ] {
            let file = directory.path().join(name);
            fs::write(&file, expected).unwrap();
            fs::set_permissions(&file, fs::Permissions::from_mode(0o644)).unwrap();
        }
        let descriptors = prepare(&path).unwrap();
        #[cfg(target_os = "linux")]
        assert_eq!(descriptors.len(), 4);
        #[cfg(target_os = "macos")]
        assert!(descriptors.len() == 1 && descriptors[0].metadata().unwrap().is_dir());
        for name in [
            "cache.sqlite",
            "cache.sqlite-wal",
            "cache.sqlite-shm",
            "cache.sqlite-journal",
        ] {
            let file = directory.path().join(name);
            assert_eq!(fs::read(&file).unwrap(), expected);
            assert_eq!(
                fs::metadata(file).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
    }

    #[test]
    fn cache_storage_private_journal_alias_rejection_preserves_all_existing_modes_and_bytes() {
        use std::os::unix::fs::symlink;
        let directory = tempfile::Builder::new()
            .permissions(fs::Permissions::from_mode(0o700))
            .tempdir()
            .unwrap();
        let path = directory.path().join("cache.sqlite");
        let outside = directory.path().join("unrelated");
        fs::write(&path, b"original database bytes").unwrap();
        fs::write(&outside, b"unrelated user bytes").unwrap();
        for file in [&path, &outside] {
            fs::set_permissions(file, fs::Permissions::from_mode(0o644)).unwrap();
        }
        let alias = directory.path().join("cache.sqlite-wal");
        symlink(&outside, &alias).unwrap();
        assert!(prepare(&path).is_err());
        assert_eq!(fs::read(&outside).unwrap(), b"unrelated user bytes");
        assert_eq!(
            fs::metadata(&outside).unwrap().permissions().mode() & 0o777,
            0o644
        );
        assert_eq!(fs::read(&path).unwrap(), b"original database bytes");
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o644
        );
        fs::remove_file(&alias).unwrap();
        fs::hard_link(&outside, &alias).unwrap();
        assert!(prepare(&path).is_err());
        assert_eq!(fs::read(&outside).unwrap(), b"unrelated user bytes");
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o644
        );
    }
}

//! Actual kernel named-zero records, never manufactured metadata fields.
use super::*;
use std::{
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Barrier,
    },
    time::{Duration, Instant},
};

#[cfg(target_os = "linux")]
#[test]
fn cache_storage_named_zero_v2_linux_unsafe_existing_optional_never_creates_absent_database() {
    for kind in ["symlink", "hardlink", "directory"] {
        let directory = directory();
        let path = directory.path().join("cache.sqlite");
        let wal = directory.path().join("cache.sqlite-wal");
        let other = directory.path().join("unrelated");
        fs::write(&other, b"original unrelated private bytes").unwrap();
        fs::set_permissions(&other, fs::Permissions::from_mode(0o644)).unwrap();
        match kind {
            "symlink" => std::os::unix::fs::symlink(&other, &wal).unwrap(),
            "hardlink" => fs::hard_link(&other, &wal).unwrap(),
            _ => fs::create_dir(&wal).unwrap(),
        }
        let original = fs::symlink_metadata(&wal).unwrap();
        assert!(!path.exists());
        assert!(prepare(&path)
            .unwrap_err()
            .to_string()
            .contains("owned regular"));
        assert!(
            !path.exists(),
            "unsafe existing {kind} caused an absent main database to be created before refusal"
        );
        let current = fs::symlink_metadata(&wal).unwrap();
        assert_eq!(
            (
                current.dev(),
                current.ino(),
                current.mode(),
                current.nlink()
            ),
            (
                original.dev(),
                original.ino(),
                original.mode(),
                original.nlink()
            )
        );
        assert_eq!(
            fs::read(&other).unwrap(),
            b"original unrelated private bytes"
        );
        assert_eq!(fs::metadata(&other).unwrap().mode() & 0o777, 0o644);
    }
}

pub(super) fn directory() -> tempfile::TempDir {
    tempfile::Builder::new()
        .permissions(fs::Permissions::from_mode(0o700))
        .tempdir()
        .unwrap()
}

pub(super) fn actual_named_zero<T>(
    path: &Path,
    mut named: impl FnMut() -> std::io::Result<Option<T>>,
    detached: impl Fn(&T) -> bool,
) -> T {
    assert!(!path.try_exists().unwrap());
    let stop = Arc::new(AtomicBool::new(false));
    let start = Arc::new(Barrier::new(2));
    let producer = {
        let (path, stop, start) = (path.to_path_buf(), stop.clone(), start.clone());
        std::thread::spawn(move || {
            start.wait();
            let mut created = 0;
            while !stop.load(Ordering::Acquire) {
                let file = fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .mode(0o600)
                    .open(&path)
                    .unwrap();
                let metadata = file.metadata().unwrap();
                assert!(metadata.is_file() && metadata.nlink() == 1);
                fs::remove_file(&path).unwrap();
                drop(file);
                created += 1;
            }
            created
        })
    };
    start.wait();
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut captured = None;
    while Instant::now() < deadline {
        if let Some(metadata) = named().unwrap() {
            if detached(&metadata) {
                captured = Some(metadata);
                break;
            }
        }
    }
    stop.store(true, Ordering::Release);
    assert!(producer.join().unwrap() > 0);
    assert!(!path.try_exists().unwrap());
    captured.expect("actual named stat never observed the required kernel zero-link race")
}

#[cfg(target_os = "linux")]
fn zero_metadata(path: &Path) -> fs::Metadata {
    actual_named_zero(
        path,
        || match fs::symlink_metadata(path) {
            Ok(metadata) => Ok(Some(metadata)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error),
        },
        |metadata| metadata.nlink() == 0,
    )
}

#[cfg(target_os = "linux")]
#[test]
fn cache_storage_named_zero_v2_linux_persistent_initial_and_current_have_one_four_pass_budget() {
    for phase in [NamedPhaseV2::Initial, NamedPhaseV2::Current] {
        let directory = directory();
        let path = directory.path().join("cache.sqlite");
        let wal = directory.path().join("cache.sqlite-wal");
        let zero = zero_metadata(&wal);
        fs::write(&path, b"original database bytes").unwrap();
        fs::write(&wal, b"original optional bytes").unwrap();
        for path in [&path, &wal] {
            fs::set_permissions(path, fs::Permissions::from_mode(0o644)).unwrap();
        }
        let mut passes = 0;
        let mut transient = 0;
        let result = prepare_with_named_metadata_v2(
            &path,
            |_, _| {},
            |name, observed_phase| {
                if name == path && observed_phase == NamedPhaseV2::Initial {
                    passes += 1;
                }
                if name == wal && observed_phase == phase {
                    transient += 1;
                    return Ok(zero.clone());
                }
                fs::symlink_metadata(name)
            },
        );
        assert_eq!((passes, transient), (4, 4));
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("bounded validation"));
        for (path, bytes) in [
            (&path, b"original database bytes".as_slice()),
            (&wal, b"original optional bytes"),
        ] {
            assert_eq!(fs::read(path).unwrap(), bytes);
            assert_eq!(fs::metadata(path).unwrap().mode() & 0o777, 0o644);
        }
    }
}

#[cfg(target_os = "linux")]
#[test]
fn cache_storage_named_zero_v2_linux_main_anchor_survives_transient_after_existing_or_created() {
    for present in [false, true] {
        for replacement in [false, true] {
            let directory = directory();
            let path = directory.path().join("cache.sqlite");
            let wal = directory.path().join("cache.sqlite-wal");
            let original = directory.path().join("preserved-original.sqlite");
            let zero = zero_metadata(&wal);
            if present {
                fs::write(&path, b"original database bytes").unwrap();
                fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
            }
            let mut injected = false;
            let result = prepare_with_named_metadata_v2(
                &path,
                |_, _| {},
                |name, phase| {
                    if name == wal
                        && phase
                            == if present {
                                NamedPhaseV2::Initial
                            } else {
                                NamedPhaseV2::Current
                            }
                        && !injected
                    {
                        injected = true;
                        fs::rename(&path, &original).unwrap();
                        if replacement {
                            fs::write(&path, b"foreign replacement bytes").unwrap();
                            fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
                        }
                        return Ok(zero.clone());
                    }
                    fs::symlink_metadata(name)
                },
            );
            assert!(injected);
            assert!(result
                .unwrap_err()
                .to_string()
                .contains("Original cache database"));
            assert_eq!(
                path.exists(),
                replacement,
                "must never recreate or rebase main"
            );
            assert_eq!(
                fs::read(&original).unwrap(),
                if present {
                    b"original database bytes".as_slice()
                } else {
                    b""
                }
            );
            assert_eq!(
                fs::metadata(&original).unwrap().mode() & 0o777,
                if present { 0o644 } else { 0o600 }
            );
            if replacement {
                assert_eq!(fs::read(&path).unwrap(), b"foreign replacement bytes");
                assert_eq!(fs::metadata(&path).unwrap().mode() & 0o777, 0o644);
            }
        }
    }
}

#[cfg(target_os = "linux")]
#[test]
fn cache_storage_named_zero_v2_linux_main_zero_and_late_aliases_remain_terminal() {
    for kind in ["main-zero", "symlink", "hardlink", "directory"] {
        let directory = directory();
        let path = directory.path().join("cache.sqlite");
        let wal = directory.path().join("cache.sqlite-wal");
        let other = directory.path().join("unrelated");
        let zero = zero_metadata(&wal);
        for file in [&path, &other] {
            fs::write(file, b"original private bytes").unwrap();
            fs::set_permissions(file, fs::Permissions::from_mode(0o644)).unwrap();
        }
        let mut injected = false;
        let result = prepare_with_named_metadata_v2(
            &path,
            |_, _| {},
            |name, phase| {
                if !injected
                    && phase == NamedPhaseV2::Initial
                    && (if kind == "main-zero" {
                        name == path
                    } else {
                        name == wal
                    })
                {
                    injected = true;
                    match kind {
                        "symlink" => std::os::unix::fs::symlink(&other, &wal).unwrap(),
                        "hardlink" => fs::hard_link(&other, &wal).unwrap(),
                        "directory" => fs::create_dir(&wal).unwrap(),
                        _ => (),
                    }
                    return Ok(zero.clone());
                }
                fs::symlink_metadata(name)
            },
        );
        assert!(injected);
        assert!(result.unwrap_err().to_string().contains("owned regular"));
        for file in [&path, &other] {
            assert_eq!(fs::read(file).unwrap(), b"original private bytes");
            assert_eq!(fs::metadata(file).unwrap().mode() & 0o777, 0o644);
        }
    }
    assert!(
        !optional_named_unlinked_v2(1, true, false, true),
        "foreign UID must never be a transient"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn cache_storage_named_zero_v2_actual_initial_and_current_snapshots_restart_whole_set() {
    for phase in [NamedPhaseV2::Initial, NamedPhaseV2::Current] {
        let directory = directory();
        let path = directory.path().join("cache.sqlite");
        let wal = directory.path().join("cache.sqlite-wal");
        let connection = rusqlite::Connection::open(&path).unwrap();
        connection.execute_batch("CREATE TABLE evidence(value TEXT); INSERT INTO evidence VALUES('original private committed bytes');").unwrap();
        let zero = actual_named_zero(
            &wal,
            || match fs::symlink_metadata(&wal) {
                Ok(metadata) => Ok(Some(metadata)),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
                Err(error) => Err(error),
            },
            |metadata| metadata.nlink() == 0,
        );
        assert!(zero.is_file() && zero.uid() == unsafe { libc::geteuid() });
        assert_eq!(zero.mode() & 0o777, 0o600);
        assert_eq!(zero.nlink(), 0);
        fs::write(&wal, b"same-owned optional fixture bytes").unwrap();
        fs::set_permissions(&wal, fs::Permissions::from_mode(0o644)).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        let mut injected = false;
        let mut passes = 0;
        let held = prepare_with_named_metadata_v2(
            &path,
            |_, _| {},
            |name, observed_phase| {
                if name == path && observed_phase == NamedPhaseV2::Initial {
                    passes += 1;
                }
                if name == wal && phase == observed_phase && !injected {
                    injected = true;
                    return Ok(zero.clone());
                }
                fs::symlink_metadata(name)
            },
        )
        .unwrap();
        assert!(injected);
        assert_eq!(
            passes, 2,
            "must discard and rebuild the complete target set"
        );
        assert_eq!(held.len(), 2);
        assert_eq!(
            fs::read(&wal).unwrap(),
            b"same-owned optional fixture bytes"
        );
        for name in [&path, &wal] {
            assert_eq!(fs::metadata(name).unwrap().mode() & 0o777, 0o600);
        }
        let actual: String = connection
            .query_row("SELECT value FROM evidence", [], |row| row.get(0))
            .unwrap();
        assert_eq!(actual, "original private committed bytes");
    }
}

#[cfg(target_os = "linux")]
#[test]
fn cache_storage_named_zero_v2_linux_actual_owner_and_link_policy_remains_strict() {
    let directory = directory();
    let path = directory.path().join("owned");
    let alias = directory.path().join("alias");
    fs::write(&path, b"original bytes").unwrap();
    let metadata = fs::symlink_metadata(&path).unwrap();
    let uid = unsafe { libc::geteuid() };
    assert_eq!(metadata.uid(), uid);
    verify(&metadata, uid).unwrap();
    // Actual owned metadata under a different requiredUID must fail solely
    // the ownership gate, without requiring privilege to chown fixture files.
    assert!(verify(&metadata, uid.checked_add(1).unwrap()).is_err());
    fs::hard_link(&path, &alias).unwrap();
    let linked = fs::symlink_metadata(&path).unwrap();
    assert_eq!(linked.nlink(), 2);
    assert!(!optional_named_unlinked_v2(
        1,
        linked.is_file(),
        linked.uid() == uid,
        linked.nlink() == 0
    ));
    assert!(verify(&linked, uid).is_err());
    let zero = zero_metadata(&directory.path().join("actual-zero"));
    assert!(!optional_named_unlinked_v2(
        1,
        zero.is_file(),
        zero.uid() == uid.checked_add(1).unwrap(),
        zero.nlink() == 0
    ));
    assert!(verify(&zero, uid.checked_add(1).unwrap()).is_err());
    assert_eq!(fs::read(&path).unwrap(), b"original bytes");
}

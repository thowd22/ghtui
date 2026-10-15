use super::*;
use crate::storage_private::{named_race_v2_tests, NamedPhaseV2};
use std::os::unix::fs::PermissionsExt;

fn phase_case(phase: NamedPhaseV2) {
    let directory = named_race_v2_tests::directory();
    let path = directory.path().join("cache.sqlite");
    let wal = directory.path().join("cache.sqlite-wal");
    let connection = rusqlite::Connection::open(&path).unwrap();
    connection.execute_batch("CREATE TABLE evidence(value TEXT); INSERT INTO evidence VALUES('original directory-policy committed bytes');").unwrap();
    let held_directory = File::open(directory.path()).unwrap();
    let name = CString::new("cache.sqlite-wal").unwrap();
    let zero = named_race_v2_tests::actual_named_zero(
        &wal,
        || stat_at(&held_directory, &name),
        |metadata| metadata.st_nlink == 0,
    );
    assert_eq!(zero.st_mode & libc::S_IFMT, libc::S_IFREG);
    assert_eq!(zero.st_uid, unsafe { libc::geteuid() });
    assert_eq!(zero.st_mode & 0o777, 0o600);
    fs::write(&wal, b"original optional bytes").unwrap();
    for path in [&path, &wal] {
        fs::set_permissions(path, fs::Permissions::from_mode(0o644)).unwrap();
    }
    let mut injected = false;
    let mut passes = 0;
    let held = prepare_with_named_metadata_v2(
        &path,
        |_| passes += 1,
        |directory, observed_name, observed_phase| {
            if observed_name == &name && observed_phase == phase && !injected {
                injected = true;
                return Ok(Some(zero));
            }
            stat_at(directory, observed_name)
        },
    )
    .unwrap();
    assert!(injected);
    assert_eq!(passes, if phase == NamedPhaseV2::Initial { 1 } else { 2 });
    assert_eq!(held.len(), 1);
    assert!(held[0].metadata().unwrap().is_dir());
    assert_eq!(fs::read(&wal).unwrap(), b"original optional bytes");
    for path in [&path, &wal] {
        assert_eq!(fs::metadata(path).unwrap().mode() & 0o777, 0o600);
    }
    let actual: String = connection
        .query_row("SELECT value FROM evidence", [], |row| row.get(0))
        .unwrap();
    assert_eq!(actual, "original directory-policy committed bytes");
}

#[test]
fn cache_storage_named_zero_v2_directory_actual_initial_snapshot() {
    phase_case(NamedPhaseV2::Initial);
}
#[test]
fn cache_storage_named_zero_v2_directory_actual_current_snapshot() {
    phase_case(NamedPhaseV2::Current);
}
#[test]
fn cache_storage_named_zero_v2_directory_actual_before_repair_snapshot() {
    phase_case(NamedPhaseV2::BeforeRepair);
}
#[test]
fn cache_storage_named_zero_v2_directory_actual_repaired_snapshot() {
    phase_case(NamedPhaseV2::Repaired);
}

fn zero_stat(directory: &File, path: &Path) -> libc::stat {
    let name = CString::new(path.file_name().unwrap().as_bytes()).unwrap();
    named_race_v2_tests::actual_named_zero(
        path,
        || stat_at(directory, &name),
        |metadata| metadata.st_nlink == 0,
    )
}

#[test]
fn cache_storage_named_zero_v2_directory_all_named_phases_share_one_four_pass_budget() {
    for phase in [
        NamedPhaseV2::Initial,
        NamedPhaseV2::Current,
        NamedPhaseV2::BeforeRepair,
        NamedPhaseV2::Repaired,
    ] {
        let directory = named_race_v2_tests::directory();
        let path = directory.path().join("cache.sqlite");
        let wal = directory.path().join("cache.sqlite-wal");
        let held_directory = File::open(directory.path()).unwrap();
        let zero = zero_stat(&held_directory, &wal);
        for path in [&path, &wal] {
            fs::write(path, b"original private bytes").unwrap();
            fs::set_permissions(path, fs::Permissions::from_mode(0o644)).unwrap();
        }
        let name = CString::new("cache.sqlite-wal").unwrap();
        let main = CString::new("cache.sqlite").unwrap();
        let mut passes = 0;
        let mut transient = 0;
        let result = prepare_with_named_metadata_v2(
            &path,
            |_| {},
            |directory, observed_name, observed_phase| {
                if observed_name == &main && observed_phase == NamedPhaseV2::Initial {
                    passes += 1;
                }
                if observed_name == &name && observed_phase == phase {
                    transient += 1;
                    return Ok(Some(zero));
                }
                stat_at(directory, observed_name)
            },
        );
        assert_eq!((passes, transient), (4, 4));
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("bounded validation"));
        for path in [&path, &wal] {
            assert_eq!(fs::read(path).unwrap(), b"original private bytes");
        }
        // A late transient can follow an already validated, intentional0600
        // repair; it must not undo that safe repair or admit an unsafe target.
        assert_eq!(
            fs::metadata(&path).unwrap().mode() & 0o777,
            if matches!(phase, NamedPhaseV2::Initial | NamedPhaseV2::Current) {
                0o644
            } else {
                0o600
            }
        );
        assert_eq!(
            fs::metadata(&wal).unwrap().mode() & 0o777,
            if phase == NamedPhaseV2::Repaired {
                0o600
            } else {
                0o644
            }
        );
    }
}

#[test]
fn cache_storage_named_zero_v2_directory_anchor_survives_transient_after_existing_or_creation() {
    for present in [false, true] {
        for replacement in [false, true] {
            let directory = named_race_v2_tests::directory();
            let path = directory.path().join("cache.sqlite");
            let wal = directory.path().join("cache.sqlite-wal");
            let original = directory.path().join("preserved-original.sqlite");
            let held_directory = File::open(directory.path()).unwrap();
            let zero = zero_stat(&held_directory, &wal);
            if present {
                fs::write(&path, b"original database bytes").unwrap();
                fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
            }
            let name = CString::new("cache.sqlite-wal").unwrap();
            let mut injected = false;
            let result = prepare_with_named_metadata_v2(
                &path,
                |_| {},
                |directory, observed_name, phase| {
                    if observed_name == &name
                        && phase == NamedPhaseV2::Initial
                        && path.exists()
                        && !injected
                    {
                        injected = true;
                        fs::rename(&path, &original).unwrap();
                        if replacement {
                            fs::write(&path, b"foreign replacement bytes").unwrap();
                            fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
                        }
                        return Ok(Some(zero));
                    }
                    stat_at(directory, observed_name)
                },
            );
            assert!(injected);
            assert!(result
                .unwrap_err()
                .to_string()
                .contains("Original cache database"));
            assert_eq!(path.exists(), replacement);
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

#[test]
fn cache_storage_named_zero_v2_directory_main_zero_late_alias_and_unsafe_before_creation_refuse() {
    for kind in ["main-zero", "symlink", "hardlink", "directory"] {
        for main_exists in [false, true] {
            if kind == "main-zero" && !main_exists {
                continue;
            }
            let directory = named_race_v2_tests::directory();
            let path = directory.path().join("cache.sqlite");
            let wal = directory.path().join("cache.sqlite-wal");
            let other = directory.path().join("unrelated");
            let held_directory = File::open(directory.path()).unwrap();
            let zero = zero_stat(&held_directory, &wal);
            if main_exists {
                fs::write(&path, b"original private bytes").unwrap();
            }
            fs::write(&other, b"original private bytes").unwrap();
            for file in [&path, &other].into_iter().filter(|p| p.exists()) {
                fs::set_permissions(file, fs::Permissions::from_mode(0o644)).unwrap();
            }
            let main = CString::new("cache.sqlite").unwrap();
            let name = CString::new("cache.sqlite-wal").unwrap();
            let mut injected = false;
            let result = prepare_with_named_metadata_v2(
                &path,
                |_| {},
                |directory, observed_name, phase| {
                    if !injected
                        && phase == NamedPhaseV2::Initial
                        && (if kind == "main-zero" {
                            observed_name == &main
                        } else {
                            observed_name == &name
                        })
                    {
                        injected = true;
                        match kind {
                            "symlink" => std::os::unix::fs::symlink(&other, &wal).unwrap(),
                            "hardlink" => fs::hard_link(&other, &wal).unwrap(),
                            "directory" => fs::create_dir(&wal).unwrap(),
                            _ => (),
                        }
                        return Ok(Some(zero));
                    }
                    stat_at(directory, observed_name)
                },
            );
            assert!(injected);
            assert!(result.unwrap_err().to_string().contains("owned regular"));
            assert_eq!(path.exists(), main_exists);
            for file in [&path, &other].into_iter().filter(|p| p.exists()) {
                assert_eq!(fs::read(file).unwrap(), b"original private bytes");
                assert_eq!(fs::metadata(file).unwrap().mode() & 0o777, 0o644);
            }
        }
    }
}

#[test]
fn cache_storage_named_zero_v2_directory_actual_owner_and_link_policy_remains_strict() {
    let directory = named_race_v2_tests::directory();
    let path = directory.path().join("owned");
    let alias = directory.path().join("alias");
    fs::write(&path, b"original bytes").unwrap();
    let held = File::open(directory.path()).unwrap();
    let name = CString::new("owned").unwrap();
    let metadata = stat_at(&held, &name).unwrap().unwrap();
    let uid = unsafe { libc::geteuid() };
    assert_eq!(metadata.st_uid, uid);
    verify(&metadata, uid).unwrap();
    assert!(verify(&metadata, uid.checked_add(1).unwrap()).is_err());
    fs::hard_link(&path, &alias).unwrap();
    let linked = stat_at(&held, &name).unwrap().unwrap();
    assert_eq!(linked.st_nlink, 2);
    assert!(!super::super::optional_named_unlinked_v2(
        1,
        linked.st_mode & libc::S_IFMT == libc::S_IFREG,
        linked.st_uid == uid,
        linked.st_nlink == 0
    ));
    assert!(verify(&linked, uid).is_err());
    let zero = zero_stat(&held, &directory.path().join("actual-zero"));
    assert!(!super::super::optional_named_unlinked_v2(
        1,
        zero.st_mode & libc::S_IFMT == libc::S_IFREG,
        zero.st_uid == uid.checked_add(1).unwrap(),
        zero.st_nlink == 0
    ));
    assert!(verify(&zero, uid.checked_add(1).unwrap()).is_err());
    assert_eq!(fs::read(&path).unwrap(), b"original bytes");
}

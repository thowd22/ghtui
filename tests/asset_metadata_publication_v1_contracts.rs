#[path = "support/asset_metadata_publication_v1.rs"]
mod publication;
use publication::complete_metadata;
use std::io::Write;

#[test]
fn related_pr_avatars_metadata_complete_exact_record_only() {
    assert_eq!(complete_metadata(b"complete\n"), Ok(true));
}

#[test]
fn related_pr_avatars_metadata_every_unpublished_prefix_is_not_ready() {
    let complete = b"complete\n";
    for end in 0..complete.len() {
        assert_eq!(complete_metadata(&complete[..end]), Ok(false));
    }
}

#[test]
fn related_pr_avatars_metadata_wrong_duplicate_control_and_oversized_records_refuse() {
    for bytes in [
        b"done\n".as_slice(),
        b"complete\ncomplete\n",
        b"complete\r\n",
        b"complete\0",
        b"COMPLETE\n",
        b"complete\nextra",
        &[b'x'; 1024],
    ] {
        assert!(complete_metadata(bytes).is_err());
    }
}

#[test]
fn related_pr_avatars_metadata_actual_open_writer_pid_and_partial_sidecars_are_not_completion() {
    let directory = tempfile::tempdir().unwrap();
    let pid = directory.path().join("pid");
    let marker = directory.path().join("metadata");
    let argv = directory.path().join("argv");
    let environment = directory.path().join("environment");
    std::fs::write(pid, format!("{}\n", std::process::id())).unwrap();
    std::fs::write(argv, b"--url\nhttps://avatars.example.invalid/u/201\n").unwrap();
    assert!(!environment.exists() && !marker.exists());
    let mut writer = std::fs::File::create(&marker).unwrap();
    writer.write_all(b"comp").unwrap();
    assert_eq!(
        complete_metadata(&std::fs::read(&marker).unwrap()),
        Ok(false)
    );
    std::fs::write(&environment, b"PATH\nLANG\n").unwrap();
    assert_eq!(
        complete_metadata(&std::fs::read(&marker).unwrap()),
        Ok(false)
    );
    writer.write_all(b"lete\n").unwrap();
    assert_eq!(
        complete_metadata(&std::fs::read(&marker).unwrap()),
        Ok(true)
    );
    assert_eq!(std::fs::read(&environment).unwrap(), b"PATH\nLANG\n");
}

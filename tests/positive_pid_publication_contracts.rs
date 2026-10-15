#[path = "support/positive_pid_publication_v1.rs"]
mod publication;
use publication::complete_positive_pid;
use std::io::Write;

#[test]
fn related_pr_avatars_pid_complete_positive_exact_decimal_record() {
    assert_eq!(complete_positive_pid(b"1\n").unwrap(), Some(1));
    assert_eq!(
        complete_positive_pid(b"2147483647\n").unwrap(),
        Some(i32::MAX)
    );
    let record = format!("{}\n", std::process::id());
    assert_eq!(
        complete_positive_pid(record.as_bytes()).unwrap(),
        Some(std::process::id() as i32)
    );
}
#[test]
fn related_pr_avatars_pid_every_partial_numeric_prefix_waits_for_actual_newline() {
    let record = format!("{}\n", std::process::id());
    for length in 0..record.len() {
        assert_eq!(
            complete_positive_pid(&record.as_bytes()[..length]).unwrap(),
            None,
            "No partial publication is process authority"
        );
    }
    assert_eq!(
        complete_positive_pid(record.as_bytes()).unwrap(),
        Some(std::process::id() as i32)
    );
}
#[test]
fn related_pr_avatars_pid_refuses_zero_negative_malformed_multiline_and_overflow() {
    for invalid in [
        b"0\n".as_slice(),
        b"-1\n",
        b"+12\n",
        b" 12\n",
        b"12 \n",
        b"12\r\n",
        b"\n",
        b"12\n13\n",
        b"12\n\n",
        b"12\0\n",
        b"2147483648\n",
        b"999999999999\n",
        b"9999999999999",
    ] {
        assert_eq!(
            complete_positive_pid(invalid).unwrap_err().kind(),
            std::io::ErrorKind::InvalidData,
            "{invalid:?}"
        );
    }
}
#[test]
fn related_pr_avatars_pid_real_file_open_writer_prefix_and_newline_publication() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("owned-pid");
    let mut writer = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .unwrap();
    assert_eq!(
        complete_positive_pid(&std::fs::read(&path).unwrap()).unwrap(),
        None
    );
    let digits = std::process::id().to_string();
    writer.write_all(digits.as_bytes()).unwrap();
    writer.flush().unwrap();
    assert_eq!(
        complete_positive_pid(&std::fs::read(&path).unwrap()).unwrap(),
        None,
        "Even actual complete digits are unpublished without newline"
    );
    writer.write_all(b"\n").unwrap();
    writer.flush().unwrap();
    assert_eq!(
        complete_positive_pid(&std::fs::read(&path).unwrap()).unwrap(),
        Some(std::process::id() as i32)
    );
}

use super::support::command_journal_v1::committed_prefix;

#[test]
fn test_framework_command_journal_inflight_append_waits_for_newline_without_losing_prefix() {
    let mut bytes = b"{\"step\":0}\n{\"step\":".to_vec();
    assert_eq!(committed_prefix(&bytes).unwrap(), "{\"step\":0}\n");
    bytes.extend_from_slice(b"1}");
    assert_eq!(committed_prefix(&bytes).unwrap().lines().count(), 1);
    bytes.push(b'\n');
    let rows = committed_prefix(&bytes)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["step"], 0);
    assert_eq!(rows[1]["step"], 1);
    assert_eq!(committed_prefix(b"{\"step\":0}").unwrap(), "");
}
#[test]
fn test_framework_command_journal_malformed_committed_record_is_not_hidden_or_accepted() {
    let bytes = b"{\"step\":0}\n{broken}\npartial";
    let prefix = committed_prefix(bytes).unwrap();
    assert_eq!(prefix.lines().count(), 2);
    assert_eq!(prefix.lines().nth(1), Some("{broken}"));
    assert!(serde_json::from_str::<serde_json::Value>(prefix.lines().nth(1).unwrap()).is_err());
}
#[test]
fn test_framework_command_journal_only_committed_utf8_is_decoded_invalid_full_rows_fail() {
    let prefix = "{\"actor\":\"界\"}\n";
    let mut bytes = prefix.as_bytes().to_vec();
    bytes.extend_from_slice(&[0xf0, 0x9f]);
    assert_eq!(committed_prefix(&bytes).unwrap(), prefix);
    bytes.push(b'\n');
    assert!(committed_prefix(&bytes).is_err());
    assert_eq!(committed_prefix(&[0xf0, 0x9f]).unwrap(), "");
}

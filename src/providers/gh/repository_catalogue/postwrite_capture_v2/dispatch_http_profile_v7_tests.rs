//! TASK54W7 source declarations only. Framing cannot mint a receipt or owner.
use super::*;

#[test]
fn scoped_actions_dispatch_v7_task54_w7_http200_uses_actual_gh_mixed_framing_not204_or201() {
    let body = r#"{"workflow_run_id":124}"#;
    for status_line in ["HTTP/2.0 200 OK\n", "HTTP/1.1 200 OK\r\n"] {
        let raw = format!("{status_line}Content-Length: {}\r\n\r\n{body}", body.len());
        let parsed = response(raw.as_bytes(), ProfileV5::ActionsDispatchCreated200).unwrap();
        assert_eq!(parsed.status, 200);
        assert_eq!(parsed.body, body);
        assert!(parsed.notification_poll.etag.is_none());
        for old in [
            ProfileV5::ReviewEmpty204,
            ProfileV5::ActionsEmpty202,
            ProfileV5::FileCommentCreated201,
        ] {
            assert!(response(raw.as_bytes(), old).is_err());
        }
    }
    for status in [201, 202, 204, 205, 304] {
        assert!(response(
            format!("HTTP/2.0 {status} Other\nContent-Length: 0\r\n\r\n").as_bytes(),
            ProfileV5::ActionsDispatchCreated200
        )
        .is_err());
    }
}

#[test]
fn scoped_actions_dispatch_v7_task54_w7_http200_body_cap_does_not_change_file201_or_notification_caps(
) {
    let body = "x".repeat(48 * 1024);
    let exact = format!(
        "HTTP/2.0 200 OK\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    );
    assert_eq!(
        response(exact.as_bytes(), ProfileV5::ActionsDispatchCreated200)
            .unwrap()
            .body
            .len(),
        48 * 1024
    );
    let too_large = format!("{exact}x");
    assert!(response(too_large.as_bytes(), ProfileV5::ActionsDispatchCreated200).is_err());
    let file = too_large.replacen("200 OK", "201 Created", 1);
    assert!(response(file.as_bytes(), ProfileV5::FileCommentCreated201).is_ok());
    assert!(response(too_large.as_bytes(), ProfileV5::NotificationPage200Or304).is_ok());
    let large = format!("HTTP/2.0 200 OK\n\n{}", "x".repeat(64 * 1024));
    assert!(response(large.as_bytes(), ProfileV5::ActionsDispatchCreated200).is_err());
}

#[test]
fn scoped_actions_dispatch_v7_task54_w7_http200_refuses_duplicate_control_interim_and_incomplete_headers(
) {
    for raw in [
        "HTTP/2.0 200 OK\nContent-Length: 1\r\nContent-Length: 1\r\n\r\nx",
        "HTTP/2.0 200 OK\nX-Bad: \u{1b}[2J\r\n\r\nx",
        "HTTP/2.0 200 OK\nX-Bad: \u{202e}\r\n\r\nx",
        "HTTP/2.0 200 OK\nX-Good: a\r\nX-Bad: b\n\r\nx",
        "HTTP/1.1 100 Continue\r\n\r\nHTTP/2.0 200 OK\n\r\nx",
        "HTTP/2.0 200 OK\nContent-Length: 1\r\n",
        "HTTP/2.0 200 OK\nContent-Length: 999999999999999999999\r\n\r\nx",
    ] {
        assert!(response(raw.as_bytes(), ProfileV5::ActionsDispatchCreated200).is_err());
    }
    let mut oversized = "HTTP/2.0 200 OK\n".to_owned();
    for _ in 0..128 {
        oversized.push_str(&format!("X-A: {}\r\n", "a".repeat(128)));
    }
    oversized.push_str("\r\nx");
    assert!(response(oversized.as_bytes(), ProfileV5::ActionsDispatchCreated200).is_err());
}

#[test]
fn scoped_actions_dispatch_v7_task54_w7_http_profile_extension_preserves_original_literal_zero_and_decoded_policies(
) {
    let double_zero = b"HTTP/2.0 204 No Content\nContent-Length: 00\r\n\r\n";
    assert!(response(double_zero, ProfileV5::ReviewEmpty204).is_err());
    assert!(response(double_zero, ProfileV5::ReactionRemove204).is_ok());
    assert!(response(
        b"HTTP/2.0 202 Accepted\nContent-Length: 0\r\n\r\n",
        ProfileV5::ActionsEmpty202
    )
    .is_ok());
    assert!(response(
        b"HTTP/2.0 205 Reset Content\nContent-Length: 0\r\n\r\n",
        ProfileV5::NotificationReadEmpty205
    )
    .is_ok());
    let cached = response(
        b"HTTP/2.0 304 Not Modified\nContent-Length: 1234\r\nETag: exact\r\n\r\n",
        ProfileV5::NotificationPage200Or304,
    )
    .unwrap();
    assert!(cached.body.is_empty());
    assert_eq!(cached.notification_poll.etag, Some("exact"));
    // Both closed created-JSON profiles see gh's decoded/sanitized body;
    // bounded wire Content-Length is deliberately not stdout-length equality.
    assert!(response(
        b"HTTP/2.0 201 Created\nContent-Length: 999\r\n\r\n{}",
        ProfileV5::FileCommentCreated201
    )
    .is_ok());
    assert!(response(
        b"HTTP/2.0 200 OK\nContent-Length: 999\r\n\r\n{}",
        ProfileV5::ActionsDispatchCreated200
    )
    .is_ok());
    assert!(response(
        b"HTTP/2.0 200 OK\nContent-Length: 999\r\n\r\n{}",
        ProfileV5::ReactionAdd200Or201
    )
    .is_err());
}

//! UNRUN private framing contracts. No parsed status/body mints a source,
//! transport permit or receipt; existing actual-producer suites remain required.
use super::*;
const ALL: [ProfileV5; 4] = [
    ProfileV5::ReviewEmpty204,
    ProfileV5::ActionsEmpty202,
    ProfileV5::ReactionAdd200Or201,
    ProfileV5::ReactionRemove204,
];
#[test]
fn scoped_http_response_v5_actual_gh_lf_status_crlf_headers_and_uniform_empty_profiles() {
    for (profile, status) in [
        (ProfileV5::ReviewEmpty204, 204),
        (ProfileV5::ActionsEmpty202, 202),
        (ProfileV5::ReactionRemove204, 204),
    ] {
        for bytes in [
            format!("HTTP/2.0 {status} Accepted\nContent-Length: 0\r\n\r\n"),
            format!("HTTP/1.1 {status} Accepted\r\nContent-Length: 0\r\n\r\n"),
            format!("HTTP/2.0 {status} Accepted\n\r\n"),
        ] {
            let parsed = response(bytes.as_bytes(), profile).unwrap();
            assert_eq!(parsed.status, status);
            assert!(parsed.body.is_empty());
        }
    }
}
#[test]
fn scoped_http_response_v5_strict_literal_zero_and_reaction_numeric_zero_preserve_distinct_legacy_policy(
) {
    let bytes = b"HTTP/2.0 204 No Content\nContent-Length: 00\r\n\r\n";
    assert!(response(bytes, ProfileV5::ReviewEmpty204).is_err());
    assert!(response(bytes, ProfileV5::ReactionRemove204).is_ok());
    assert!(response(
        b"HTTP/2.0 202 Accepted\nContent-Length: 00\n\n",
        ProfileV5::ActionsEmpty202
    )
    .is_err());
    assert!(response(bytes, ProfileV5::ReactionAdd200Or201).is_err());
}
#[test]
fn scoped_http_response_v5_reaction_remove_keeps128k_total_with16k_headers_not_stricter_review16k_total(
) {
    let mut bytes = format!("HTTP/2.0 204 {}\n", "r".repeat(900));
    for _ in 0..128 {
        bytes.push_str(&format!("X-A: {}\n", "a".repeat(120)));
    }
    bytes.push('\n');
    assert!(bytes.len() > 16 * 1024 && bytes.len() < 128 * 1024);
    assert!(response(bytes.as_bytes(), ProfileV5::ReactionRemove204).is_ok());
    assert!(response(bytes.as_bytes(), ProfileV5::ReviewEmpty204).is_err());
}
#[test]
fn scoped_http_response_v5_every_profile_refuses_interim_transfer_duplicate_control_mixed_and_trailing_frames(
) {
    for profile in ALL {
        let status = if matches!(profile, ProfileV5::ActionsEmpty202) {
            202
        } else if profile.add() {
            201
        } else {
            204
        };
        for suffix in [
            "Content-Length: 0\nContent-Length: 0\n\n",
            "Transfer-Encoding: chunked\n\n",
            "X-Bad: \u{1b}[2J\n\n",
            "X-Bad: \u{202e}\n\n",
            "X-Good: yes\r\nX-Bad: mixed\n\r\n",
        ] {
            assert!(response(
                format!("HTTP/2.0 {status} OK\n{suffix}").as_bytes(),
                profile
            )
            .is_err());
        }
        assert!(response(
            format!("HTTP/1.1 100 Continue\r\n\r\nHTTP/2.0 {status} OK\n\n").as_bytes(),
            profile
        )
        .is_err());
        if !profile.add() {
            assert!(response(
                format!("HTTP/2.0 {status} OK\n\nHTTP/2.0 {status} OK\n\n").as_bytes(),
                profile
            )
            .is_err());
        }
    }
}
#[test]
fn scoped_http_response_v5_json_profiles_keep_exact200_201_utf8_byte_length_without_relaxing_empty_statuses(
) {
    let body = "{\"label\":\"界\"}";
    for code in [200, 201] {
        let bytes = format!(
            "HTTP/2.0 {code} OK\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        );
        let parsed = response(bytes.as_bytes(), ProfileV5::ReactionAdd200Or201).unwrap();
        assert_eq!(parsed.status, code);
        assert_eq!(parsed.body, body);
        assert!(response(bytes.as_bytes(), ProfileV5::ReviewEmpty204).is_err());
        assert!(response(bytes.as_bytes(), ProfileV5::ReactionRemove204).is_err());
        assert!(response(
            format!(
                "HTTP/2.0 {code} OK\nContent-Length: {}\n\n{body}",
                body.len() + 1
            )
            .as_bytes(),
            ProfileV5::ReactionAdd200Or201
        )
        .is_err());
    }
    for code in [202, 204, 302, 404] {
        assert!(response(
            format!("HTTP/2.0 {code} Other\n\n{{}}").as_bytes(),
            ProfileV5::ReactionAdd200Or201
        )
        .is_err());
    }
}
#[test]
fn scoped_http_response_v5_actual_status_header_line_count_and_total_cap_refusals() {
    for profile in ALL {
        assert!(response(&vec![b'x'; 128 * 1024 + 1], profile).is_err());
        let status = if matches!(profile, ProfileV5::ActionsEmpty202) {
            202
        } else if profile.add() {
            201
        } else {
            204
        };
        for headers in [
            "X-H: x\n".repeat(129),
            format!("X-H: {}\n", "x".repeat(4097)),
        ] {
            assert!(response(
                format!("HTTP/2.0 {status} OK\n{headers}\n").as_bytes(),
                profile
            )
            .is_err());
        }
        assert!(response(
            format!("HTTP/2.0 {status} {}\n\n", "x".repeat(1024)).as_bytes(),
            profile
        )
        .is_err());
        assert!(response(b"HTTP/2.0 204 OK\nX-Bad: \xff\n\n", profile).is_err());
    }
}
#[test]
fn scoped_http_response_v5_borrows_actual_body_slice_and_does_not_clone_or_parse_json_in_header_grammar(
) {
    let bytes = b"HTTP/2.0 201 Created\nContent-Length: 2\r\n\r\n{}";
    let parsed = response(bytes, ProfileV5::ReactionAdd200Or201).unwrap();
    assert_eq!(parsed.body.as_ptr(), bytes[bytes.len() - 2..].as_ptr());
    assert_eq!(parsed.body, "{}");
    // Existing purpose caller still enforces its one JSON object/receipt shape.
    assert_eq!(
        response(
            b"HTTP/2.0 201 Created\n\n[]",
            ProfileV5::ReactionAdd200Or201
        )
        .unwrap()
        .body,
        "[]"
    );
}
#[test]
fn scoped_http_response_v5_existing_review_failure_class_and_diagnostics_are_byte_preserved() {
    let failure =
        super::super::empty_http_ack_v5::validate(b"HTTP/2.0 204 OK\n\nnull", "204").unwrap_err();
    assert!(matches!(failure.kind, GithubErrorKind::AmbiguousWrite));
    assert_eq!(
        failure.message,
        "HTTP204 must have no content; trailing payload is not a receipt"
    );
    let failure =
        super::super::empty_http_ack_v5::validate(b"HTTP/2.0 200 OK\n\n", "204").unwrap_err();
    assert_eq!(failure.message,"Review comment DELETE requires exact documented HTTP204; 200/404/null is not an acknowledgement");
    assert!(matches!(
        super::super::empty_http_ack_v5::validate(b"", "200")
            .unwrap_err()
            .kind,
        GithubErrorKind::InvalidTarget
    ));
}

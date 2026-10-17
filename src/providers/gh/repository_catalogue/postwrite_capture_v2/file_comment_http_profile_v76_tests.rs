//! Root composition regressions for ONE grammar with closed purpose policies.
use super::*;
fn file(raw: &[u8]) -> GithubResult<ResponseV5<'_>> {
    response(raw, ProfileV5::FileCommentCreated201)
}
#[test]
fn file_comments_v75_shared_http_file201_keeps_actual_status_body_and_header_delimiter() {
    for raw in [
        "HTTP/2.0 201 Created\nContent-Type: application/json\r\n\r\n{\"id\":91}",
        "HTTP/1.1 201 Created\r\nContent-Type: application/json\r\n\r\n{\"id\":91}",
        "HTTP/2 201 Created\n\n{\"id\":91}",
    ] {
        let parsed = file(raw.as_bytes()).unwrap();
        assert_eq!(parsed.status, 201);
        assert_eq!(parsed.body, "{\"id\":91}");
    }
    let raw = "HTTP/2 201 Created\nX-Safe: value\n\n{\r\n\"id\":91\r\n}";
    assert_eq!(file(raw.as_bytes()).unwrap().body, "{\r\n\"id\":91\r\n}");
    for code in ["200", "202", "204", "304", "0201", "2010"] {
        assert!(file(format!("HTTP/2 {code} Observed\n\n{{}}").as_bytes()).is_err());
    }
}
#[test]
fn file_comments_v75_shared_http_decoded_file_length_never_weakens_old_reaction_or_empty_profiles()
{
    for length in ["0", "999", "99999999999999999999"] {
        let raw = format!("HTTP/2 201 Created\nContent-Length: {length}\r\n\r\n{{}}");
        assert_eq!(file(raw.as_bytes()).unwrap().body, "{}");
        assert!(response(raw.as_bytes(), ProfileV5::ReactionAdd200Or201).is_err());
    }
    for length in ["", "-1", "+2", "2 2", "999999999999999999999"] {
        assert!(file(
            format!("HTTP/2 201 Created\nContent-Length: {length}\r\n\r\n{{}}").as_bytes()
        )
        .is_err());
    }
    let duplicate = b"HTTP/2 201 Created\nContent-Length: 2\r\ncontent-length: 2\r\n\r\n{}";
    assert!(file(duplicate).is_err());
    let decoded = b"HTTP/2 201 Created\nTransfer-Encoding: chunked\r\n\r\n{}";
    assert_eq!(file(decoded).unwrap().body, "{}");
    for profile in [
        ProfileV5::ReactionAdd200Or201,
        ProfileV5::ReactionRemove204,
        ProfileV5::ReviewEmpty204,
        ProfileV5::ActionsEmpty202,
    ] {
        assert!(response(decoded, profile).is_err());
    }
    assert!(response(
        b"HTTP/2 204 No Content\nContent-Length: 00\r\n\r\n",
        ProfileV5::ReviewEmpty204
    )
    .is_err());
    assert!(response(
        b"HTTP/2 204 No Content\nContent-Length: 00\r\n\r\n",
        ProfileV5::ReactionRemove204
    )
    .is_ok());
}
#[test]
fn file_comments_v75_shared_http_combined_header_and_body_bounds_are_not_two_hidden_pools() {
    let at = format!("HTTP/2 201 Created\n\n{}", "x".repeat(240 * 1024));
    assert_eq!(file(at.as_bytes()).unwrap().body.len(), 240 * 1024);
    assert!(file(format!("{at}x").as_bytes()).is_err());
    for count in [128, 129] {
        let raw = format!("HTTP/2 201 Created\n{}\r\n{{}}", "X: x\r\n".repeat(count));
        assert_eq!(file(raw.as_bytes()).is_ok(), count == 128);
    }
    let header = format!(
        "HTTP/2 201 Created\n{}\r\n{{}}",
        format!("X: {}\r\n", "x".repeat(4090)).repeat(4)
    );
    assert!(header.len() > 16 * 1024);
    assert!(file(header.as_bytes()).is_err());
    assert!(file(&vec![b'x'; 256 * 1024 + 1]).is_err());
    assert!(file(b"HTTP/2 201 Created\n\n").is_err());
}
#[test]
fn file_comments_v75_shared_http_refuses_mixed_controls_interim_and_missing_boundaries_without_echo(
) {
    for raw in [
        b"HTTP/2 201 Created\nX-Safe: first\nX-Other: second\r\n\r\n{}".as_slice(),
        b"HTTP/2 201 Created\r\nX-Safe: first\n\n{}".as_slice(),
        b"HTTP/2 201 Created\nX-Bad: \x1b[2J-private-sentinel\r\n\r\n{}".as_slice(),
        b"HTTP/2 201 Created\nX-Bad: \xff\r\n\r\n{}".as_slice(),
        b"HTTP/2 201 Created\n folded: private-sentinel\r\n\r\n{}".as_slice(),
        b"HTTP/2 100 Continue\n\r\nHTTP/2 201 Created\n\r\n{}".as_slice(),
        b"HTTP/2 201 Created\nX-Safe: incomplete".as_slice(),
    ] {
        let failure = match file(raw) {
            Ok(_) => panic!("Malformed fixture admitted"),
            Err(error) => error,
        };
        assert!(!format!("{failure:?}").contains("private-sentinel"));
    }
}

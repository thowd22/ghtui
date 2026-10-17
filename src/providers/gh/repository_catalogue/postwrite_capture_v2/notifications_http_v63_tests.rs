use super::*;

#[test]
fn notifications_http_v63_exact_exit_status_pairs_and_representation_length() {
    for raw in [
        "HTTP/1.1 304 Not Modified\n\r\n",
        "HTTP/2 304 Not Modified\nContent-Length: 17\r\nETag: \"abc\"\r\n\r\n",
    ] {
        assert!(matches!(
            page(raw.as_bytes(), 1).unwrap(),
            NotificationHttpPageV63::NotModified { .. }
        ));
        for exit in [-1, 0, 2, 130] {
            assert!(page(raw.as_bytes(), exit).is_err());
        }
    }
    let fresh = b"HTTP/2.0 200 OK\nContent-Length: 2\r\nX-Poll-Interval: 60\r\n\r\n[]";
    match page(fresh, 0).unwrap() {
        NotificationHttpPageV63::Fresh { body, poll } => {
            assert_eq!(body, "[]");
            assert_eq!(poll.poll_interval, Some("60"));
        }
        _ => panic!("genuine fresh response was misclassified"),
    }
    assert!(page(fresh, 1).is_err());
}

#[test]
fn notifications_http_v63_wrong_status_payload_injection_duplicates_and_controls_refuse() {
    for raw in [
        "HTTP/2 304 Not Modified\n\n[]",
        "HTTP/2 304 Not Modified\nContent-Length: 262145\n\n",
        "HTTP/2 304 Not Modified\nContent-Length: 00\n\n",
        "HTTP/2 304 Not Modified\nTransfer-Encoding: chunked\n\n",
        "HTTP/2 200 OK\nContent-Length: 123456789012345678901\n\n[]",
        "HTTP/2 304 Not Modified\nETag: a\nETag: b\n\n",
        "HTTP/2 304 Not Modified\nX-Poll-Interval: 15\nx-poll-interval: 16\n\n",
        "HTTP/2 304 Not Modified\nLast-Modified: a\nLast-Modified: b\n\n",
        "HTTP/2 304 Not Modified\nLink: x\nLINK: y\n\n",
        "HTTP/2 304 Not Modified\nRetry-After: 30\nRetry-After: 31\n\n",
        "HTTP/2 304 Not Modified\nX-Bad: \u{1b}[2J\n\n",
        "HTTP/1.1 100 Continue\n\nHTTP/2 304 Not Modified\n\n",
        "HTTP/2 401 Unauthorized\n\n",
        "HTTP/2 429 Rate Limited\n\n",
    ] {
        for exit in [0, 1] {
            assert!(
                page(raw.as_bytes(), exit).is_err(),
                "accepted malformed protocol with exit {exit}"
            );
        }
    }
}

#[test]
fn notifications_http_v63_page200_only_accepts_bounded_decoded_json_framing() {
    for raw in [
        "HTTP/2 200 OK\nContent-Length: 39\r\n\r\n[]",
        "HTTP/2 200 OK\nTransfer-Encoding: chunked\r\n\r\n[]",
        "HTTP/2 200 OK\nTransfer-Encoding: ChUnKeD\r\n\r\n[]",
    ] {
        let NotificationHttpPageV63::Fresh { body, .. } = page(raw.as_bytes(), 0).unwrap() else {
            panic!("wrong phase");
        };
        assert_eq!(serde_json::from_str::<Value>(body).unwrap(), json!([]));
        assert!(page(raw.as_bytes(), 1).is_err());
    }
    for raw in [
        "HTTP/2 200 OK\nTransfer-Encoding: gzip\n\n[]",
        "HTTP/2 200 OK\nTransfer-Encoding: \n\n[]",
        "HTTP/2 200 OK\nTransfer-Encoding: gzip, chunked\n\n[]",
        "HTTP/2 200 OK\nTransfer-Encoding: chunked\nTransfer-Encoding: chunked\n\n[]",
        "HTTP/2 200 OK\nTransfer-Encoding: chunked\nContent-Length: 2\n\n[]",
        "HTTP/2 200 OK\nContent-Length: 2\nContent-Length: 2\n\n[]",
    ] {
        assert!(page(raw.as_bytes(), 0).is_err());
    }
    // Old reaction/Review policies are NOT relaxed by a read-only200 profile.
    let raw = b"HTTP/2 200 OK\nTransfer-Encoding: chunked\n\n{}";
    assert!(
        super::super::pinned_http_response_v5::response(raw, ProfileV5::ReactionAdd200Or201)
            .is_err()
    );
    assert!(super::super::pinned_http_response_v5::response(
        b"HTTP/2 204 No Content\nTransfer-Encoding: chunked\n\n",
        ProfileV5::ReviewEmpty204
    )
    .is_err());
}

#[test]
fn notifications_http_v63_mark_read_requires_exact205_empty_not204_or304() {
    assert!(read_ack(b"HTTP/2 205 Reset Content\nContent-Length: 0\r\n\r\n").is_ok());
    for raw in [
        "HTTP/2 204 No Content\n\n",
        "HTTP/2 304 Not Modified\n\n",
        "HTTP/2 205 Reset Content\n\n{}",
        "HTTP/2 205 Reset Content\nContent-Length: 00\n\n",
    ] {
        assert!(read_ack(raw.as_bytes()).is_err());
    }
}

#[test]
fn notifications_http_v63_subscription_object_and_exact_total_header_caps() {
    assert_eq!(
        subscription(b"HTTP/2 200 OK\nContent-Length: 2\n\n{}").unwrap(),
        json!({})
    );
    for raw in [
        "HTTP/2 201 Created\n\n{}",
        "HTTP/2 200 OK\n\n[]",
        "HTTP/2 200 OK\n\nnull",
        "HTTP/2 200 OK\n\n{}{}",
    ] {
        assert!(subscription(raw.as_bytes()).is_err());
    }
    let mut large = b"HTTP/2 200 OK\n\n".to_vec();
    large.resize(256 * 1024 + 1, b' ');
    assert!(page(&large, 0).is_err());
    assert!(subscription(&large).is_err());
    let mut headers = String::from("HTTP/2 304 Not Modified\n");
    for n in 0..5 {
        headers.push_str(&format!("X-{n}: {}\n", "a".repeat(3500)));
    }
    headers.push('\n');
    assert!(page(headers.as_bytes(), 1).is_err());
}

#[test]
fn notifications_http_v63_poll_observations_borrow_raw_and_debug_is_redacted() {
    let raw = "HTTP/2 200 OK\nLast-Modified: Thu, 15 Oct 2026 18:00:00 GMT\r\nETag: \"private-validator\"\r\nLink: <https://api.github.com/notifications?page=2>; rel=\"next\"\r\nRetry-After: 120\r\n\r\n[]";
    let parsed = page(raw.as_bytes(), 0).unwrap();
    let debug = format!("{parsed:?}");
    assert!(!debug.contains("private-validator"));
    assert!(!debug.contains("api.github.com"));
    let NotificationHttpPageV63::Fresh { body, poll } = parsed else {
        panic!("wrong response kind");
    };
    assert_eq!(poll.retry_after, Some("120"));
    assert_eq!(poll.etag, Some("\"private-validator\""));
    assert!(poll.last_modified.is_some());
    assert!(poll.link.is_some());
    let begin = raw.as_ptr() as usize;
    let end = begin + raw.len();
    assert!((begin..end).contains(&(body.as_ptr() as usize)));
    assert!((begin..end).contains(&(poll.etag.unwrap().as_ptr() as usize)));
}

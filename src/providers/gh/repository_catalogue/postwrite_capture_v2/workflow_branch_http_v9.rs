//! Same pinned HTTP framing; a404 is descriptive API unavailability only.
use super::*;
pub(in crate::providers::gh) fn workflow_branch_response_v9(
    bytes: Vec<u8>,
    exit: i32,
    capture: crate::providers::gh::WorkflowJsonCaptureV9,
) -> GithubResult<Option<crate::providers::gh::WorkflowJsonValueV9>> {
    response_v9(
        bytes,
        exit,
        capture,
        pinned_http_response_v5::ProfileV5::WorkflowBranch200Or404V9,
        16 * 1024,
    )
}
pub(in crate::providers::gh) fn workflow_history_response_v9(
    bytes: Vec<u8>,
    exit: i32,
    capture: crate::providers::gh::WorkflowJsonCaptureV9,
) -> GithubResult<Option<crate::providers::gh::WorkflowJsonValueV9>> {
    response_v9(
        bytes,
        exit,
        capture,
        pinned_http_response_v5::ProfileV5::WorkflowHistory200Or404V9,
        128 * 1024,
    )
}
fn response_v9(
    bytes: Vec<u8>,
    exit: i32,
    capture: crate::providers::gh::WorkflowJsonCaptureV9,
    profile: pinned_http_response_v5::ProfileV5,
    cap: usize,
) -> GithubResult<Option<crate::providers::gh::WorkflowJsonValueV9>> {
    let refused = || {
        error(
            GithubErrorKind::Transport,
            "Original exact branch HTTP observation unavailable; no absence inferred",
        )
    };
    let response = pinned_http_response_v5::response(&bytes, profile)?;
    if response.body.is_empty()
        || response.body.len() > cap
        || !matches!((response.status, exit), (200, 0) | (404, 1))
    {
        return Err(refused());
    }
    let status = response.status;
    let start = bytes
        .len()
        .checked_sub(response.body.len())
        .ok_or_else(refused)?;
    let length = bytes.len();
    let raw = capture.decode_owned_v9(bytes, start..length)?;
    if !(raw.value().is_object()
        || status == 200
            && matches!(
                profile,
                pinned_http_response_v5::ProfileV5::WorkflowHistory200Or404V9
            )
            && raw.value().is_array())
    {
        return Err(refused());
    }
    if status == 404 {
        if raw.value().get("message").and_then(Value::as_str) != Some("Not Found") {
            return Err(refused());
        }
        Ok(None)
    } else {
        Ok(Some(raw))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn workflow_branch_response_v9(bytes: &[u8], exit: i32) -> GithubResult<Option<Value>> {
        super::workflow_branch_response_v9(
            bytes.to_vec(),
            exit,
            crate::providers::gh::WorkflowJsonLedgerV9::test_response_capture_v9(16 * 1024),
        )
        .map(|read| read.map(|value| value.value().clone()))
    }
    #[test]
    fn task54_workflow_edit_v9_exact_branch_http_requires_actual_documented_exit_status_pair() {
        let absent = b"HTTP/2 404 Not Found\nContent-Length: 23\r\n\r\n{\"message\":\"Not Found\"}";
        assert!(workflow_branch_response_v9(absent, 1).unwrap().is_none());
        for exit in [-1, 0, 2, 130] {
            assert!(workflow_branch_response_v9(absent, exit).is_err());
        }
        let present = b"HTTP/2 200 OK\nContent-Length: 2\r\n\r\n{}";
        assert_eq!(
            workflow_branch_response_v9(present, 0).unwrap(),
            Some(json!({}))
        );
        assert!(workflow_branch_response_v9(present, 1).is_err());
    }
    #[test]
    fn task54_workflow_edit_v9_raw404_never_accepts_diagnostic_auth_null_duplicate_or_transferred_body(
    ) {
        for raw in [
            "gh: Not Found (HTTP 404)",
            "HTTP/2 401 Unauthorized\n\n{\"message\":\"Not Found\"}",
            "HTTP/2 403 Forbidden\n\n{\"message\":\"Not Found\"}",
            "HTTP/2 404 Not Found\n\nnull",
            "HTTP/2 404 Not Found\n\n[]",
            "HTTP/2 404 Not Found\n\n<html>login</html>",
            "HTTP/2 404 Not Found\n\n{\"message\":\"Not Found\"}trailing",
            "HTTP/2 404 Not Found\nContent-Length: 23\nContent-Length: 23\n\n{\"message\":\"Not Found\"}",
            "HTTP/2 404 Not Found\nTransfer-Encoding: chunked\n\n{\"message\":\"Not Found\"}",
            "HTTP/2 100 Continue\n\nHTTP/2 404 Not Found\n\n{\"message\":\"Not Found\"}",
            "HTTP/2 404 Not Found\nX-Bad: \u{1b}[2J\n\n{\"message\":\"Not Found\"}",
        ] {assert!(workflow_branch_response_v9(raw.as_bytes(),1).is_err());}
    }
    #[test]
    fn task54_workflow_edit_v9_raw_branch_capture_bound_counts_unused_json_fields() {
        let raw = format!(
            "HTTP/2 404 Not Found\n\n{{\"message\":\"Not Found\",\"unused\":\"{}\"}}",
            "x".repeat(16 * 1024)
        );
        assert!(workflow_branch_response_v9(raw.as_bytes(), 1).is_err());
        assert!(workflow_branch_response_v9(
            b"HTTP/2 404 Not Found\nContent-Length: 1\n\n{\"message\":\"Not Found\"}",
            1
        )
        .is_err());
    }
    #[test]
    fn task54_workflow_edit_v9_branch_profile_does_not_relax_existing_write_or_notification_profiles(
    ) {
        use super::super::pinned_http_response_v5::{response, ProfileV5};
        let raw = b"HTTP/2 404 Not Found\nContent-Length: 23\r\n\r\n{\"message\":\"Not Found\"}";
        for profile in [
            ProfileV5::ReviewEmpty204,
            ProfileV5::ReactionRemove204,
            ProfileV5::ActionsEmpty202,
            ProfileV5::NotificationPage200Or304,
            ProfileV5::ActionsDispatchCreated200,
        ] {
            assert!(response(raw, profile).is_err());
        }
        assert!(response(
            b"HTTP/2 204 No Content\nContent-Length: 00\n\n",
            ProfileV5::ReactionRemove204
        )
        .is_ok());
        assert!(response(
            b"HTTP/2 204 No Content\nContent-Length: 00\n\n",
            ProfileV5::ReviewEmpty204
        )
        .is_err());
    }
}

//! Closed review-comment DELETE transport. HTTP acknowledgement alone is NOT
//! a deletion receipt; the caller must additionally obtain exact-node null
//! readback and close the unchanged original private-visibility bracket.
use super::*;
#[cfg(test)]
#[path = "http_envelope_compat_v75_tests.rs"]
mod file_compat_v75_tests;

pub(crate) struct ReviewCommentDeleteAckV2 {
    comment_id: u64,
}
impl ReviewCommentDeleteAckV2 {
    pub(crate) fn comment_id(&self) -> u64 {
        self.comment_id
    }
}

fn exact_204(bytes: &[u8]) -> GithubResult<()> {
    super::empty_http_ack_v5::validate(bytes, "204")
}
#[cfg(test)]
pub(super) fn exact_204_for_test_v107(bytes: &[u8]) -> GithubResult<()> {
    exact_204(bytes)
}

impl PostWriteCaptureAllowanceV2 {
    /// One CLOSED operation, not a generic raw API/permission constructor.
    /// The typed source producer must provide exact original parent and ID.
    pub(crate) fn review_comment_delete_ack_v2(
        &mut self,
        provider: &GithubProvider,
        original: &PrResourceParentV1,
        id: u64,
        cancel: &CancellationToken,
    ) -> GithubResult<ReviewCommentDeleteAckV2> {
        if id == 0 || id > i64::MAX as u64 || original.summary().pr.host != provider.host {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Actual review-comment ID and original host required",
            ));
        }
        let bounded = self.spend_ordinary_slot_v2(provider, cancel)?;
        let (deadline, cap) = bounded
            .scoped_read_window
            .expect("actual fixed slot has original window");
        let bounded = bounded.with_scoped_read_window(deadline, cap.min(16 * 1024))?;
        let endpoint = format!("repos/{}/pulls/comments/{id}", original.repository());
        let resource = provider.resource(original.repository(), original.number());
        let result = (|| {
            let bytes = bounded.execute(
                vec![
                    "api".into(),
                    endpoint,
                    "--hostname".into(),
                    provider.host.clone(),
                    "--method".into(),
                    "DELETE".into(),
                    "--include".into(),
                ],
                None,
                Some(resource.clone()),
                &self.cancellation,
            )?;
            self.check_issuer(provider, cancel)?;
            exact_204(&bytes)?;
            self.check_issuer(provider, cancel)?;
            Ok(ReviewCommentDeleteAckV2 { comment_id: id })
        })();
        if result.is_err() {
            provider.runner.mark_ambiguous(&resource);
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn review_threads_v1_comment_delete_ack_requires_exact_complete_204_without_body() {
        for value in [
            "HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nX-Request-Id: observed\r\n\r\n",
            "HTTP/2.0 204 No Content\n\n",
        ] {
            assert!(exact_204(value.as_bytes()).is_ok());
        }
        for value in [
            "",
            "null",
            "HTTP/2.0 200 OK\n\n",
            "HTTP/2.0 404 Not Found\n\n",
            "HTTP/2.0 204 No Content\n",
            "HTTP/2.0 204 No Content\n\nnull",
            "HTTP/2.0 204 No Content\nContent-Length: 1\n\n",
            "HTTP/2.0 204 No Content\nContent-Length: 0\nContent-Length: 0\n\n",
            "HTTP/2.0 204 No Content\nTransfer-Encoding: chunked\n\n",
            "HTTP/2.0 204 No Content\n folded:bad\n\n",
            "HTTP/2.0 204 No Content\nX-Request: \u{1b}[2J\n\n",
            "HTTP/2.0 204 No Content\r\nX-Bad: value\nMore: mixed\r\n\r\n",
            "HTTP/1.1 100 Continue\r\n\r\nHTTP/2.0 204 No Content\r\n\r\n",
        ] {
            assert!(
                exact_204(value.as_bytes()).is_err(),
                "Not a closed204 ack: {value:?}"
            );
        }
    }
    #[test]
    fn review_threads_v1_comment_delete_ack_bounds_headers_and_rejects_non_utf8() {
        assert!(exact_204(b"HTTP/2.0 204 No Content\nX-Test: \xff\n\n").is_err());
        let many = format!("HTTP/2.0 204 No Content\n{}\n", "X-Small: x\n".repeat(129));
        assert!(exact_204(many.as_bytes()).is_err());
        let long = format!("HTTP/2.0 204 No Content\nX-Long: {}\n\n", "x".repeat(4096));
        assert!(exact_204(long.as_bytes()).is_err());
        assert!(exact_204(&vec![b'x'; 16 * 1024 + 1]).is_err());
    }
    #[test]
    fn review_threads_v3_comment_delete_ack_accepts_exact_gh_status_lf_header_crlf_only() {
        // Pinned official source byte structure, not a deletion receipt.
        for value in [
            "HTTP/2.0 204 No Content\nContent-Length: 0\r\nX-Github-Request-Id: actual-observation\r\n\r\n",
            "HTTP/1.1 204 No Content\nDate: Thu, 15 Oct 2026 09:00:00 GMT\r\n\r\n",
            "HTTP/2.0 204 No Content\n\r\n",
        ] { assert!(exact_204(value.as_bytes()).is_ok(), "{value:?}"); }
        for value in [
            "HTTP/2.0 204 No Content\nContent-Length: 0\r\n",
            "HTTP/2.0 204 No Content\nContent-Length: 0\r\n\r\nnull",
            "HTTP/2.0 200 OK\nContent-Length: 0\r\n\r\n",
            "HTTP/2.0 204 No Content\nContent-Length: 0\nX-Bad: mixed\r\n\r\n",
            "HTTP/2.0 204 No Content\nContent-Length: 0\r\nX-Bad: mixed\n\n",
            "HTTP/2.0 204 No Content\r\nContent-Length: 0\n\n",
            "HTTP/2.0 204 No Content\nContent-Length: 0\r\nContent-Length: 0\r\n\r\n",
            "HTTP/2.0 204 No Content\nX-Bad: \u{1b}[2J\r\n\r\n",
            "HTTP/2.0 204 No Content\nHTTP/2.0 204 No Content\r\n\r\n",
            "HTTP/1.1 100 Continue\n\r\nHTTP/2.0 204 No Content\n\r\n",
        ] {
            assert!(exact_204(value.as_bytes()).is_err(), "{value:?}");
        }
        assert!(exact_204(
            format!(
                "HTTP/2.0 204 No Content\nX-Large: {}\r\n\r\n",
                "x".repeat(4097)
            )
            .as_bytes()
        )
        .is_err());
    }
}

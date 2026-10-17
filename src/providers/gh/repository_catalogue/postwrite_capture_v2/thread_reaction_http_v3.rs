//! Purpose-only HTTP facts for review-comment reaction writes. These facts
//! alone are NOT receipts: exact paged actor readback must also agree.
use super::*;

pub(crate) enum ThreadReactionHttpRequestV3<'a> {
    Add(&'a str),
    Remove(u64),
}
pub(crate) enum ThreadReactionHttpAckV3 {
    Added { status: u16, value: Value },
    Removed { reaction_id: u64 },
}
fn bad() -> GithubError {
    error(GithubErrorKind::AmbiguousWrite, "Reaction HTTP acknowledgement is malformed, incomplete or outside the exact documented status/body shape")
}
fn response(bytes: &[u8], add: bool) -> GithubResult<(u16, Option<Value>)> {
    let profile = if add {
        super::pinned_http_response_v5::ProfileV5::ReactionAdd200Or201
    } else {
        super::pinned_http_response_v5::ProfileV5::ReactionRemove204
    };
    let parsed = super::pinned_http_response_v5::response(bytes, profile)?;
    if !add {
        return Ok((parsed.status, None));
    }
    let value: Value = serde_json::from_str(parsed.body).map_err(|_| bad())?;
    if !value.is_object() {
        return Err(bad());
    }
    Ok((parsed.status, Some(value)))
}
impl PostWriteCaptureAllowanceV2 {
    pub(crate) fn thread_reaction_write_ack_v3(
        &mut self,
        provider: &GithubProvider,
        original: &PrResourceParentV1,
        comment_id: u64,
        request: ThreadReactionHttpRequestV3<'_>,
        cancel: &CancellationToken,
    ) -> GithubResult<ThreadReactionHttpAckV3> {
        if !(1..=i64::MAX as u64).contains(&comment_id)
            || original.summary().pr.host != provider.host
        {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Actual original comment/host required for reaction transport",
            ));
        }
        let (endpoint, method, input, add, reaction_id) = match request {
            ThreadReactionHttpRequestV3::Add(content)
                if crate::providers::gh::ThreadReactionContentV3::parse(content).is_some() =>
            {
                (
                    format!(
                        "repos/{}/pulls/comments/{comment_id}/reactions",
                        original.repository()
                    ),
                    "POST",
                    Some(json!({"content":content})),
                    true,
                    0,
                )
            }
            ThreadReactionHttpRequestV3::Remove(id) if (1..=i64::MAX as u64).contains(&id) => (
                format!(
                    "repos/{}/pulls/comments/{comment_id}/reactions/{id}",
                    original.repository()
                ),
                "DELETE",
                None,
                false,
                id,
            ),
            _ => {
                return Err(error(
                    GithubErrorKind::InvalidTarget,
                    "Known actual reaction content/positive ID required",
                ))
            }
        };
        let bounded = self.spend_ordinary_slot_v2(provider, cancel)?;
        let (deadline, cap) = bounded
            .scoped_read_window
            .expect("original fixed slot window");
        let bounded = bounded.with_scoped_read_window(deadline, cap.min(128 * 1024))?;
        let mut args = vec![
            "api".into(),
            endpoint,
            "--hostname".into(),
            provider.host.clone(),
            "--method".into(),
            method.into(),
            "--include".into(),
        ];
        if input.is_some() {
            args.extend(["--input".into(), "-".into()]);
        }
        let resource = provider.resource(original.repository(), original.number());
        let result = (|| {
            let bytes = bounded.execute(args, input, Some(resource.clone()), &self.cancellation)?;
            self.check_issuer(provider, cancel)?;
            let (status, value) = response(&bytes, add)?;
            self.check_issuer(provider, cancel)?;
            if add {
                Ok(ThreadReactionHttpAckV3::Added {
                    status,
                    value: value.expect("exact JSON acknowledgement"),
                })
            } else {
                Ok(ThreadReactionHttpAckV3::Removed { reaction_id })
            }
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
    fn review_threads_v3_reaction_http_observes_existing_vs_created_and_exact_empty204() {
        for (status, code) in [("200 OK", 200), ("201 Created", 201)] {
            let bytes = format!("HTTP/2.0 {status}\r\nContent-Length: 2\r\n\r\n{{}}");
            assert_eq!(
                response(bytes.as_bytes(), true).unwrap(),
                (code, Some(json!({})))
            );
            assert!(response(bytes.as_bytes(), false).is_err());
        }
        for bytes in [
            b"HTTP/2.0 204 No Content\nContent-Length: 0\n\n".as_slice(),
            b"HTTP/2.0 204 No Content\nContent-Length: 0\r\n\r\n",
            b"HTTP/2.0 204 No Content\n\r\n",
        ] {
            assert_eq!(response(bytes, false).unwrap(), (204, None));
        }
        for bytes in [
            "HTTP/2.0 404 Not Found\n\n",
            "HTTP/2.0 204 No Content\n\nnull",
            "HTTP/2.0 204 No Content\nContent-Length: 0\nContent-Length: 0\n\n",
            "HTTP/2.0 204 No Content\nTransfer-Encoding: chunked\n\n",
            "HTTP/2.0 204 No Content\r\nX-Bad: one\nTwo: mixed\r\n\r\n",
            "HTTP/2.0 204 No Content\nX-Bad: \u{1b}[2J\n\n",
            "HTTP/1.1 100 Continue\n\nHTTP/2.0 204 No Content\n\n",
        ] {
            assert!(response(bytes.as_bytes(), false).is_err(), "{bytes:?}");
        }
        for bytes in [
            "HTTP/2.0 201 Created\n\n[]",
            "HTTP/2.0 201 Created\n\nnull",
            "HTTP/2.0 201 Created\nContent-Length: 3\n\n{}",
            "HTTP/2.0 201 Created\n\n{}{}",
        ] {
            assert!(response(bytes.as_bytes(), true).is_err());
        }
    }
}

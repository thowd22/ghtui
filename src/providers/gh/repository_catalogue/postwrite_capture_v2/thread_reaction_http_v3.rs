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
    if bytes.is_empty() || bytes.len() > 128 * 1024 {
        return Err(bad());
    }
    let raw = std::str::from_utf8(bytes).map_err(|_| bad())?;
    // gh processResponse prints status with Fprintln (LF), printHeaders with
    // CRLF, then one CRLF. Accept that exact producer shape as well as uniform
    // HTTP fixtures; do not normalize mixed line breaks inside header values.
    let (status, remaining) = raw.split_once('\n').ok_or_else(bad)?;
    let status_crlf = status.ends_with('\r');
    let status = status.strip_suffix('\r').unwrap_or(status);
    let separator = if remaining.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    if status_crlf && separator != "\r\n" {
        return Err(bad());
    }
    let (headers, body) = if let Some(body) = remaining.strip_prefix(separator) {
        ("", body)
    } else {
        remaining
            .split_once(&format!("{separator}{separator}"))
            .ok_or_else(bad)?
    };
    if headers.len() > 16 * 1024 {
        return Err(bad());
    }
    let lines = headers.split(separator).filter(|line| !line.is_empty());
    if status.len() > 1024 || status.bytes().any(|b| !matches!(b, 0x20..=0x7e)) {
        return Err(bad());
    }
    let mut parts = status.splitn(3, ' ');
    if !matches!(
        parts.next(),
        Some("HTTP/1.0" | "HTTP/1.1" | "HTTP/2" | "HTTP/2.0" | "HTTP/3" | "HTTP/3.0")
    ) {
        return Err(bad());
    }
    let status = match parts.next() {
        Some("200") if add => 200,
        Some("201") if add => 201,
        Some("204") if !add => 204,
        _ => return Err(bad()),
    };
    let mut length = None;
    for (index, line) in lines.enumerate() {
        if index >= 128 || line.len() > 4096 {
            return Err(bad());
        }
        let (name, value) = line.split_once(':').ok_or_else(bad)?;
        if name.is_empty()
            || !name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b))
            || value.bytes().any(|b| !matches!(b, b'\t' | 0x20..=0x7e))
        {
            return Err(bad());
        }
        if name.eq_ignore_ascii_case("content-length") {
            let raw = value.trim();
            if length.is_some() || raw.is_empty() || !raw.bytes().all(|b| b.is_ascii_digit()) {
                return Err(bad());
            }
            length = Some(raw.parse::<usize>().map_err(|_| bad())?);
        }
        // gh --include emits decoded JSON, not a wire chunk stream. We do not
        // infer a complete acknowledgement from transfer-encoded ambiguity.
        if name.eq_ignore_ascii_case("transfer-encoding") {
            return Err(bad());
        }
    }
    if length.is_some_and(|length| length != body.len()) {
        return Err(bad());
    }
    if !add {
        return if body.is_empty() {
            Ok((status, None))
        } else {
            Err(bad())
        };
    }
    let value: Value = serde_json::from_str(body).map_err(|_| bad())?;
    if !value.is_object() {
        return Err(bad());
    }
    Ok((status, Some(value)))
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

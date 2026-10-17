//! One closed FILE comment POST201 on the existing original prepaid cursor.
//! Status alone is not a receipt; GET + exact FILE node remain mandatory.
use super::*;
pub(crate) struct FileCommentCreatedAckV75 {
    value: Value,
}
impl FileCommentCreatedAckV75 {
    pub(crate) fn value(&self) -> &Value {
        &self.value
    }
}
fn json_201(bytes: &[u8]) -> GithubResult<Value> {
    if bytes.len() > 256 * 1024 {
        return Err(error(
            GithubErrorKind::AmbiguousWrite,
            "File comment HTTP201 capture exceeds original slot",
        ));
    }
    let envelope = super::pinned_http_response_v5::response(
        bytes,
        super::pinned_http_response_v5::ProfileV5::FileCommentCreated201,
    )?;
    // gh/Go may decode/sanitize the payload: Content-Length is NOT an output
    // length oracle. The bounded actual capture + complete JSON parser + later
    // typed field correspondence are required instead.
    let value: Value = serde_json::from_str(envelope.body).map_err(|_| {
        error(
            GithubErrorKind::AmbiguousWrite,
            "File comment HTTP201 JSON is incomplete or malformed",
        )
    })?;
    if !value.is_object() || value.get("errors").is_some() || value.get("message").is_some() {
        return Err(error(
            GithubErrorKind::AmbiguousWrite,
            "File comment acknowledgement is not a comment JSON object",
        ));
    }
    Ok(value)
}
impl PostWriteCaptureAllowanceV2 {
    pub(crate) fn file_comment_created201_v75(
        &mut self,
        provider: &GithubProvider,
        original: &OriginalPrFileCommentV75,
        body: &FileCommentBodyV75,
        cancel: &CancellationToken,
    ) -> GithubResult<FileCommentCreatedAckV75> {
        let parent = original.parent();
        if parent.summary().pr.host != provider.host
            || parent.number() == 0
            || parent.repository_id() == 0
        {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Actual original file comment parent required",
            ));
        }
        let input = body.payload(original)?;
        let ordinary = original.ordinary_transport_v75(provider, cancel)?;
        let bounded = self.spend_ordinary_slot_v2(&ordinary, cancel)?;
        let resource = provider.resource(parent.repository(), parent.number());
        let result = (|| {
            let raw = bounded.execute(
                vec![
                    "api".into(),
                    format!(
                        "repos/{}/pulls/{}/comments",
                        parent.repository(),
                        parent.number()
                    ),
                    "--hostname".into(),
                    provider.host.clone(),
                    "--method".into(),
                    "POST".into(),
                    "--include".into(),
                    "--input".into(),
                    "-".into(),
                ],
                Some(input),
                Some(resource.clone()),
                &self.cancellation,
            )?;
            self.check_issuer(provider, cancel)?;
            let value = json_201(&raw)?;
            self.check_issuer(provider, cancel)?;
            Ok(FileCommentCreatedAckV75 { value })
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
    fn file_comments_v75_http201_uses_actual_status_and_complete_bounded_json_not_body_guess() {
        for raw in [
            "HTTP/2.0 201 Created\nContent-Type: application/json\r\n\r\n{\"id\":91}",
            "HTTP/1.1 201 Created\r\nContent-Type: application/json\r\n\r\n{\"id\":91}",
            "HTTP/2 201 Created\n\n{\"id\":91}",
        ] {
            assert_eq!(json_201(raw.as_bytes()).unwrap()["id"], 91);
        }
        for raw in [
            "{\"id\":91}",
            "HTTP/2.0 200 OK\n\n{\"id\":91}",
            "HTTP/2.0 202 Accepted\n\n{\"id\":91}",
            "HTTP/2.0 204 No Content\n\n",
            "HTTP/2.0 201 Created\n\nnull",
            "HTTP/2.0 201 Created\n\n[]",
            "HTTP/2.0 201 Created\n\n{\"id\":",
            "HTTP/2.0 201 Created\n\n{}{}",
            "HTTP/2.0 201 Created\n\n{\"errors\":[],\"id\":91}",
            "HTTP/2.0 201 Created\nX-Header: \u{1b}[2J\r\n\r\n{\"id\":91}",
            "HTTP/2.0 201 Created\nX-Header: mixed\nAnother: value\r\n\r\n{\"id\":91}",
            "HTTP/2.0 201 Created\n\r\nHTTP/2.0 201 Created\n\r\n{\"id\":91}",
        ] {
            assert!(json_201(raw.as_bytes()).is_err(), "{raw:?}");
        }
        assert!(json_201(
            format!(
                "HTTP/2.0 201 Created\nX-Long: {}\r\n\r\n{{}}",
                "x".repeat(4097)
            )
            .as_bytes()
        )
        .is_err());
        assert!(json_201(&vec![b'x'; 256 * 1024 + 1]).is_err());
    }
}

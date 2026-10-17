//! Closed empty acknowledgements on the ONE pinned framing grammar. No raw
//! transport/status getter or reaction policy is exposed by this wrapper.
use super::*;
pub(super) fn validate(bytes: &[u8], expected: &str) -> GithubResult<()> {
    let profile = match expected {
        "204" => super::pinned_http_response_v5::ProfileV5::ReviewEmpty204,
        "202" => super::pinned_http_response_v5::ProfileV5::ActionsEmpty202,
        _ => {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "No closed empty HTTP acknowledgement for this operation",
            ))
        }
    };
    super::pinned_http_response_v5::response(bytes, profile).map(|_| ())
}

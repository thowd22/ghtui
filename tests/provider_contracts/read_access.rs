//! Read denial is typed; offline/error prose cannot erase private membership.
use super::*;
use ghtui::providers::{gh::GithubError, runner::CommandFailure};

#[test]
fn collaboration_index_read_access_classifies_actual_gh_status_envelopes() {
    for (diagnostic, status) in [
        (
            "gh: Resource not accessible by integration (HTTP 403)\n",
            403,
        ),
        ("HTTP 403: missing repository permission", 403),
        ("gh: Not Found (HTTP 404)\n", 404),
        (
            "HTTP 404 (https://github.invalid/api/v3/repos/example/repo)",
            404,
        ),
        ("HTTP 401: Bad credentials", 401),
        ("gh: API rate limit exceeded (HTTP 403)", 429),
        ("gh: Too many requests (HTTP 429)", 429),
    ] {
        let fixture = Fixture::new();
        let mut read = step(api_args("repos/example/repo"), Value::Null);
        read.status = 1;
        read.stderr = diagnostic.as_bytes().to_vec();
        fixture.scripted_gh(&[read]);
        let error = provider(&fixture)
            .repository("example/repo", &CancellationToken::default())
            .unwrap_err();
        match status {
            403 => assert!(matches!(error.kind, GithubErrorKind::Permission)),
            404 => assert!(matches!(error.kind, GithubErrorKind::NotFound)),
            401 => assert!(matches!(error.kind, GithubErrorKind::Authentication)),
            _ => assert!(matches!(error.kind, GithubErrorKind::RateLimited)),
        }
        assert_eq!(error.repository_access_lost(), matches!(status, 403 | 404));
        let journal = journal(&fixture);
        assert_eq!(journal.len(), 1);
        assert_eq!(journal[0]["matched"], true);
        assert_eq!(journal[0]["args"], json!(api_args("repos/example/repo")));
    }
}

#[test]
fn collaboration_index_read_access_numbers_and_quoted_markers_are_not_denials() {
    for diagnostic in [
        "dial tcp 127.0.0.1:403: connection refused",
        "request https://github.invalid/repos/org/repo401 failed offline",
        "transport failed for object 40412345",
        "network failed: example status HTTP 403: not a gh envelope",
        "HTTP 4030: unrelated diagnostic",
        "HTTP 404file: unrelated diagnostic",
        "{\"message\":\"HTTP 403: quoted text\"}",
        "gh: failed reading https://proxy.invalid/HTTP404 (offline)",
        "gh: resource named (HTTP 403) was not fetched",
        "gh: HTTP 403: server text is not the response status (HTTP 502)",
    ] {
        let fixture = Fixture::new();
        let mut read = step(api_args("repos/example/repo"), Value::Null);
        read.status = 1;
        read.stderr = diagnostic.as_bytes().to_vec();
        fixture.scripted_gh(&[read]);
        let error = provider(&fixture)
            .repository("example/repo", &CancellationToken::default())
            .unwrap_err();
        assert!(
            matches!(error.kind, GithubErrorKind::Transport),
            "{diagnostic}: {error:?}"
        );
        assert!(!error.repository_access_lost());
        assert_eq!(journal(&fixture).len(), 1);
        assert_eq!(journal(&fixture)[0]["matched"], true);
    }
}

#[test]
fn collaboration_index_read_access_cancel_timeout_and_uncertain_write_are_not_revocations() {
    for kind in [
        FailureKind::Cancelled,
        FailureKind::Timeout,
        FailureKind::OutputLimit,
        FailureKind::Spawn,
    ] {
        let error = GithubError::from(CommandFailure {
            kind,
            message: "failed while reading a request".into(),
            exit_code: Some(1),
            stdout: vec![],
            stderr: b"gh: Not Found (HTTP 404)".to_vec(),
            outcome_unknown: false,
        });
        assert!(matches!(error.kind, GithubErrorKind::Transport));
        assert!(!error.repository_access_lost());
    }
    let error = GithubError::from(CommandFailure {
        kind: FailureKind::Exit,
        message: "write has an unknown outcome".into(),
        exit_code: Some(1),
        stdout: vec![],
        stderr: b"gh: permission denied (HTTP 403)".to_vec(),
        outcome_unknown: true,
    });
    assert!(matches!(error.kind, GithubErrorKind::AmbiguousWrite));
    assert!(!error.repository_access_lost());
    let explicit = GithubError {
        kind: GithubErrorKind::Permission,
        message: "Typed capability denied".into(),
    };
    assert!(explicit.repository_access_lost());
}

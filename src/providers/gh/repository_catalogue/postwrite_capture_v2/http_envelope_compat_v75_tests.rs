//! Test-only byte-derived baseline comparator, NOT a second production parser.
use super::*;
fn legacy_exact_204(bytes: &[u8]) -> GithubResult<()> {
    if bytes.is_empty() || bytes.len() > 16 * 1024 {
        return Err(error(
            GithubErrorKind::AmbiguousWrite,
            "Review comment DELETE acknowledgement exceeds its closed header bound",
        ));
    }
    let raw = std::str::from_utf8(bytes).map_err(|_| {
        error(
            GithubErrorKind::AmbiguousWrite,
            "Review comment DELETE returned non-UTF8 HTTP headers",
        )
    })?;
    // cli/cli v2.101.0 processResponse prints LF status (fmt.Fprintln),
    // printHeaders writes CRLF header lines, then a final CRLF. Accept that
    // exact producer framing without normalizing mixed header-value bytes.
    let missing_boundary = || {
        error(
            GithubErrorKind::AmbiguousWrite,
            "Review comment DELETE omitted the complete HTTP header boundary",
        )
    };
    let (status, remaining) = raw.split_once('\n').ok_or_else(missing_boundary)?;
    let status_crlf = status.ends_with('\r');
    let status = status.strip_suffix('\r').unwrap_or(status);
    let separator = if remaining.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    if status_crlf && separator != "\r\n" {
        return Err(missing_boundary());
    }
    let (headers, body) = if let Some(body) = remaining.strip_prefix(separator) {
        ("", body)
    } else {
        remaining
            .split_once(&format!("{separator}{separator}"))
            .ok_or_else(missing_boundary)?
    };
    if !body.is_empty() {
        return Err(error(
            GithubErrorKind::AmbiguousWrite,
            "HTTP204 must have no content; trailing payload is not a receipt",
        ));
    }
    let lines = headers.split(separator).filter(|line| !line.is_empty());
    if status.len() > 1024 || status.bytes().any(|b| !matches!(b, 0x20..=0x7e)) {
        return Err(error(
            GithubErrorKind::AmbiguousWrite,
            "Malformed review-comment HTTP status line",
        ));
    }
    let mut parts = status.splitn(3, ' ');
    if !matches!(
        parts.next(),
        Some("HTTP/1.0" | "HTTP/1.1" | "HTTP/2" | "HTTP/2.0" | "HTTP/3" | "HTTP/3.0")
    ) || parts.next() != Some("204")
    {
        return Err(error(GithubErrorKind::AmbiguousWrite, "Review comment DELETE requires exact documented HTTP204; 200/404/null is not an acknowledgement"));
    }
    let mut count = 0;
    let mut length_seen = false;
    for line in lines {
        count += 1;
        if count > 128 || line.len() > 4096 {
            return Err(error(
                GithubErrorKind::AmbiguousWrite,
                "Review-comment HTTP header count/line bound exceeded",
            ));
        }
        let (name, value) = line.split_once(':').ok_or_else(|| {
            error(
                GithubErrorKind::AmbiguousWrite,
                "Malformed review-comment HTTP header",
            )
        })?;
        if name.is_empty()
            || !name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b))
            || value.bytes().any(|b| !matches!(b, b'\t' | 0x20..=0x7e))
        {
            return Err(error(
                GithubErrorKind::AmbiguousWrite,
                "Invalid review-comment HTTP header bytes",
            ));
        }
        if name.eq_ignore_ascii_case("content-length") {
            if length_seen || value.trim() != "0" {
                return Err(error(
                    GithubErrorKind::AmbiguousWrite,
                    "HTTP204 content length is not uniquely zero",
                ));
            }
            length_seen = true;
        }
        if name.eq_ignore_ascii_case("transfer-encoding") {
            return Err(error(
                GithubErrorKind::AmbiguousWrite,
                "HTTP204 must not advertise a transferred body",
            ));
        }
    }
    Ok(())
}

#[test]
fn file_comments_v75_existing204_acceptance_is_unchanged_for_actual_and_adversarial_framing() {
    for proto in [
        "HTTP/1.0", "HTTP/1.1", "HTTP/2", "HTTP/2.0", "HTTP/3", "HTTP/3.0", "HTTP/9",
    ] {
        for code in ["204", "201", "200", "404", "0204"] {
            for status_newline in ["\n", "\r\n"] {
                for sep in ["\n", "\r\n"] {
                    for header in [
                        "",
                        "Content-Length: 0",
                        "Content-Length: 1",
                        "Content-Length: 0\nContent-Length: 0",
                        "Transfer-Encoding: chunked",
                        "X-Safe: actual",
                        " folded:bad",
                        "X-Bad: \u{1b}[2J",
                    ] {
                        for body in ["", "null", "\n", "HTTP/2.0 204 No Content\r\n\r\n"] {
                            let raw = format!(
                                "{proto} {code} Observed{status_newline}{}{sep}{body}",
                                if header.is_empty() {
                                    String::new()
                                } else {
                                    format!("{}{sep}", header.replace("\n", sep))
                                }
                            );
                            assert_eq!(
                                exact_204(raw.as_bytes()).is_ok(),
                                legacy_exact_204(raw.as_bytes()).is_ok(),
                                "{raw:?}"
                            );
                        }
                    }
                }
            }
        }
    }
}
#[test]
fn file_comments_v75_existing204_capture_header_count_and_utf8_bounds_remain_exact() {
    for length in [0, 1, 4095, 4096, 4097, 16380, 16384, 16385] {
        let raw = format!(
            "HTTP/2.0 204 No Content\nX-Actual: {}\r\n\r\n",
            "x".repeat(length)
        );
        assert_eq!(
            exact_204(raw.as_bytes()).is_ok(),
            legacy_exact_204(raw.as_bytes()).is_ok()
        );
    }
    for count in [0, 1, 127, 128, 129] {
        let raw = format!(
            "HTTP/2.0 204 No Content\n{}\r\n",
            "X-Actual: x\r\n".repeat(count)
        );
        assert_eq!(
            exact_204(raw.as_bytes()).is_ok(),
            legacy_exact_204(raw.as_bytes()).is_ok()
        );
    }
    for raw in [
        b"HTTP/2.0 204 No Content\nX-Bad: \xff\r\n\r\n".as_slice(),
        b"HTTP/2.0 204 No Content\n\r\n\xff".as_slice(),
    ] {
        assert_eq!(exact_204(raw).is_ok(), legacy_exact_204(raw).is_ok());
    }
}

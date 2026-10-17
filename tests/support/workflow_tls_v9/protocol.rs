//! Closed smart-HTTP grammar. Never retain auth, payload, pack or private keys
//! in request metadata; never proxy a supplied URL or arbitrary CGI header.
use super::*;
use std::os::unix::io::AsRawFd;

pub(super) const HEADER_CAP: usize = 16 * 1024;
pub(super) const STREAM_CAP: usize = 1024 * 1024;
pub(super) struct Request {
    pub method: &'static str,
    pub path: &'static str,
    pub path_info: &'static str,
    pub query: &'static str,
    pub content_type: &'static str,
    pub protocol_v2: bool,
    pub body: Vec<u8>,
    pub bytes: usize,
}

type ParsedHeaders<'a> = (&'a str, Vec<(&'a str, &'a str)>);
fn headers(bytes: &[u8]) -> Result<ParsedHeaders<'_>, String> {
    if bytes.len() > HEADER_CAP || !bytes.ends_with(b"\r\n\r\n") {
        return Err("Fixture request header boundary/bound refused".into());
    }
    let text = std::str::from_utf8(bytes).map_err(|_| "Fixture request headers are not UTF8")?;
    let mut lines = text[..text.len() - 4].split("\r\n");
    let first = lines
        .next()
        .filter(|s| !s.is_empty() && s.len() <= 1024)
        .ok_or("Fixture request line unavailable")?;
    let mut fields = Vec::new();
    for line in lines {
        if fields.len() >= 64 || line.len() > 4096 {
            return Err("Fixture request header count/line bound refused".into());
        }
        let (key, value) = line
            .split_once(':')
            .ok_or("Malformed fixture request header")?;
        if key.is_empty()
            || !key
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b))
            || value.bytes().any(|b| !matches!(b, b'\t' | 0x20..=0x7e))
            || fields
                .iter()
                .any(|(old, _): &(&str, &str)| old.eq_ignore_ascii_case(key))
        {
            return Err("Duplicate/control fixture request header refused".into());
        }
        fields.push((key, value.trim()));
    }
    Ok((first, fields))
}

pub(super) fn connect(bytes: &[u8]) -> Result<(), String> {
    let (first, fields) = headers(bytes)?;
    if first != "CONNECT ghe.example:443 HTTP/1.1" {
        return Err("Only exact canonical fixture CONNECT is permitted".into());
    }
    let mut host = false;
    for (key, value) in fields {
        if key.eq_ignore_ascii_case("host") {
            host = value == "ghe.example:443";
        } else if !["user-agent", "proxy-connection", "connection"]
            .iter()
            .any(|v| key.eq_ignore_ascii_case(v))
        {
            return Err("Unexpected/credential fixture CONNECT header refused".into());
        }
    }
    if !host {
        return Err("Canonical fixture CONNECT Host missing".into());
    }
    Ok(())
}

pub(super) fn parse(bytes: &[u8], body: Vec<u8>) -> Result<Request, String> {
    let (first, fields) = headers(bytes)?;
    let (method, path, path_info, query, content_type) = match first {
        "GET /example/ci.git/info/refs?service=git-upload-pack HTTP/1.1" => (
            "GET",
            "/example/ci.git/info/refs?service=git-upload-pack",
            "/example/ci.git/info/refs",
            "service=git-upload-pack",
            "",
        ),
        "GET /example/ci.git/info/refs?service=git-receive-pack HTTP/1.1" => (
            "GET",
            "/example/ci.git/info/refs?service=git-receive-pack",
            "/example/ci.git/info/refs",
            "service=git-receive-pack",
            "",
        ),
        "POST /example/ci.git/git-upload-pack HTTP/1.1" => (
            "POST",
            "/example/ci.git/git-upload-pack",
            "/example/ci.git/git-upload-pack",
            "",
            "application/x-git-upload-pack-request",
        ),
        "POST /example/ci.git/git-receive-pack HTTP/1.1" => (
            "POST",
            "/example/ci.git/git-receive-pack",
            "/example/ci.git/git-receive-pack",
            "",
            "application/x-git-receive-pack-request",
        ),
        _ => return Err("Noncanonical smart-HTTP method/path refused".into()),
    };
    let (mut host, mut length, mut content, mut protocol_v2) = (false, None, None, false);
    for (key, value) in fields {
        if key.eq_ignore_ascii_case("host") {
            host = value == "ghe.example" || value == "ghe.example:443";
        } else if key.eq_ignore_ascii_case("content-length") {
            if value.is_empty() || value.len() > 7 || !value.bytes().all(|b| b.is_ascii_digit()) {
                return Err("Invalid bounded smart-HTTP content length".into());
            }
            length = Some(
                value
                    .parse::<usize>()
                    .map_err(|_| "Invalid smart-HTTP content length")?,
            );
        } else if key.eq_ignore_ascii_case("content-type") {
            content = Some(value);
        } else if key.eq_ignore_ascii_case("git-protocol") {
            if value != "version=2" {
                return Err("Unsupported Git-Protocol fixture value".into());
            }
            protocol_v2 = true;
        } else if ![
            "user-agent",
            "accept",
            "accept-encoding",
            "accept-language",
            "pragma",
            "cache-control",
            "connection",
        ]
        .iter()
        .any(|v| key.eq_ignore_ascii_case(v))
        {
            // In particular no Authorization, cookies, Expect/continue,
            // forwarded environment or transferred/unknown-length body.
            return Err("Unexpected/credential smart-HTTP header refused".into());
        }
    }
    if !host
        || body.len() > STREAM_CAP
        || length.unwrap_or(0) != body.len()
        || method == "GET" && (!body.is_empty() || content.is_some())
        || method == "POST" && (content != Some(content_type) || length.is_none())
    {
        return Err("Smart-HTTP host/content/bounded body differs".into());
    }
    Ok(Request {
        method,
        path,
        path_info,
        query,
        content_type,
        protocol_v2,
        bytes: bytes
            .len()
            .checked_add(body.len())
            .ok_or("Smart-HTTP size overflow")?,
        body,
    })
}

pub(super) fn read_headers<R: Read + AsRawFd>(
    reader: &mut R,
    stop: &AtomicBool,
    deadline: Instant,
) -> Result<Vec<u8>, String> {
    // One byte at a time ONLY for the bounded header, so no TLS/request/pack
    // bytes are accidentally consumed by the CONNECT/header parser.
    let mut bytes = Vec::with_capacity(1024);
    while !bytes.ends_with(b"\r\n\r\n") {
        if bytes.len() == HEADER_CAP {
            return Err("Smart-HTTP header bound exceeded".into());
        }
        process::wait_fd(reader.as_raw_fd(), libc::POLLIN, stop, deadline)?;
        let mut byte = [0];
        match reader.read(&mut byte) {
            Ok(1) => bytes.push(byte[0]),
            Ok(0) => return Err("Smart-HTTP peer closed before complete headers".into()),
            Ok(_) => unreachable!(),
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                ) => {}
            Err(_) => return Err("Smart-HTTP header read failed".into()),
        }
    }
    Ok(bytes)
}

pub(super) fn read_request(
    reader: &mut std::process::ChildStdout,
    stop: &AtomicBool,
    deadline: Instant,
) -> Result<Request, String> {
    let header = read_headers(reader, stop, deadline)?;
    let (_, fields) = headers(&header)?;
    let length = fields
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("content-length"))
        .map(|(_, n)| n.parse::<usize>())
        .transpose()
        .map_err(|_| "Invalid smart-HTTP body length")?
        .unwrap_or(0);
    if length > STREAM_CAP {
        return Err("Smart-HTTP body exceeds1MiB".into());
    }
    // A body allocation is strictly bounded before reading. Credential and
    // transfer framing refuse now; full closed path/header/body semantics
    // must still pass below BEFORE an actual backend can be invoked.
    let empty = parse(&header, Vec::new());
    if length == 0 {
        return empty;
    }
    // No fake body or successful response is created for this framing check.
    validate_pending_headers(&header, length)?;
    let mut body = vec![0; length];
    let mut at = 0;
    while at < body.len() {
        process::wait_fd(reader.as_raw_fd(), libc::POLLIN, stop, deadline)?;
        match reader.read(&mut body[at..]) {
            Ok(0) => return Err("Truncated smart-HTTP body".into()),
            Ok(n) => at += n,
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                ) => {}
            Err(_) => return Err("Smart-HTTP body read failed".into()),
        }
    }
    parse(&header, body)
}

fn validate_pending_headers(header: &[u8], length: usize) -> Result<(), String> {
    // A zero-sized temporary body cannot pass the real length check. Instead
    // count/validate framing before the bounded body; full semantics are
    // validated against exact bytes again before backend invocation.
    let (first, fields) = headers(header)?;
    if !first.starts_with("POST /example/ci.git/git-")
        || length > STREAM_CAP
        || fields.iter().any(|(key, _)| {
            key.eq_ignore_ascii_case("authorization")
                || key.eq_ignore_ascii_case("proxy-authorization")
                || key.eq_ignore_ascii_case("transfer-encoding")
        })
    {
        return Err("Pending smart-HTTP header refused".into());
    }
    Ok(())
}

pub(super) fn response(cgi: &[u8]) -> Result<(u16, Vec<u8>), String> {
    if cgi.len() > STREAM_CAP {
        return Err("CGI capture exceeded1MiB".into());
    }
    let split = cgi
        .windows(4)
        .position(|s| s == b"\r\n\r\n")
        .ok_or("CGI omitted complete CRLF header boundary")?;
    let head = &cgi[..split];
    if head.len() > HEADER_CAP {
        return Err("CGI headers exceeded16KiB".into());
    }
    let text = std::str::from_utf8(head).map_err(|_| "CGI headers are not UTF8")?;
    let mut status = 200;
    let mut rows = Vec::new();
    for line in text.split("\r\n") {
        if rows.len() >= 64 || line.len() > 4096 {
            return Err("CGI header count/line exceeded".into());
        }
        let (key, value) = line.split_once(':').ok_or("Malformed CGI header")?;
        if key.is_empty()
            || !key
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b))
            || value.bytes().any(|b| !matches!(b, b'\t' | 0x20..=0x7e))
            || rows
                .iter()
                .any(|(k, _): &(&str, &str)| k.eq_ignore_ascii_case(key))
        {
            return Err("Malformed/duplicate CGI header".into());
        }
        if key.eq_ignore_ascii_case("status") {
            status = match value.trim() {
                "200 OK" => 200,
                "403 Forbidden" => 403,
                "404 Not Found" => 404,
                _ => return Err("Unsupported CGI status".into()),
            };
        }
        if [
            "connection",
            "transfer-encoding",
            "content-length",
            "www-authenticate",
            "set-cookie",
        ]
        .iter()
        .any(|v| key.eq_ignore_ascii_case(v))
        {
            return Err("Unexpected CGI framing/auth header".into());
        }
        rows.push((key, value));
    }
    let body = &cgi[split + 4..];
    let mut out = format!(
        "HTTP/1.1 {status} {}\r\nConnection: close\r\nContent-Length: {}\r\n",
        match status {
            200 => "OK",
            403 => "Forbidden",
            _ => "Not Found",
        },
        body.len()
    )
    .into_bytes();
    for (key, value) in rows {
        if !key.eq_ignore_ascii_case("status") {
            out.extend_from_slice(format!("{key}:{value}\r\n").as_bytes());
        }
    }
    out.extend_from_slice(b"\r\n");
    out.extend_from_slice(body);
    if out.len() > STREAM_CAP {
        return Err("HTTP response exceeds1MiB including framing".into());
    }
    Ok((status, out))
}

#[cfg(test)]
pub(super) mod contract_hooks {
    use super::*;
    pub fn task54_workflow_tls_v58_connect_refuses_foreign_authority_credentials_and_duplicate_host(
    ) {
        assert!(
            connect(b"CONNECT ghe.example:443 HTTP/1.1\r\nHost: ghe.example:443\r\n\r\n").is_ok()
        );
        for raw in ["CONNECT github.com:443 HTTP/1.1\r\nHost: github.com:443\r\n\r\n", "CONNECT ghe.example:443 HTTP/1.1\r\nHost: ghe.example:443\r\nProxy-Authorization: x\r\n\r\n", "CONNECT ghe.example:443 HTTP/1.1\r\nHost: ghe.example:443\r\nHost: ghe.example:443\r\n\r\n"] { assert!(connect(raw.as_bytes()).is_err()); }
    }
    pub fn task54_workflow_tls_v58_only_exact_git_paths_and_bounded_protocol2_reach_backend() {
        let valid=b"POST /example/ci.git/git-upload-pack HTTP/1.1\r\nHost: ghe.example\r\nContent-Type: application/x-git-upload-pack-request\r\nContent-Length: 4\r\nGit-Protocol: version=2\r\n\r\n";
        let request = parse(valid, b"0000".to_vec()).unwrap();
        assert!(request.protocol_v2);
        assert_eq!(request.path_info, "/example/ci.git/git-upload-pack");
        for (from, to) in [
            ("/example/ci.git/", "/other/ci.git/"),
            ("version=2", "version=9"),
            ("Content-Length: 4", "Transfer-Encoding: chunked"),
            ("Host: ghe.example", "Authorization: secret"),
        ] {
            assert!(parse(
                String::from_utf8_lossy(valid).replace(from, to).as_bytes(),
                b"0000".to_vec()
            )
            .is_err());
        }
        assert!(parse(valid, vec![0; STREAM_CAP + 1]).is_err());
    }
    pub fn task54_workflow_tls_v58_actual_cgi_body_is_unchanged_but_auth_and_ambiguous_framing_refuse(
    ) {
        let (status,bytes)=response(b"Expires: Fri, 01 Jan 1980 00:00:00 GMT\r\nContent-Type: application/x-git-receive-pack-result\r\n\r\n0000\0pack").unwrap();
        assert_eq!(status, 200);
        assert!(bytes.ends_with(b"0000\0pack"));
        for raw in [
            b"Content-Length: 0\r\n\r\n".as_slice(),
            b"Set-Cookie: secret\r\n\r\n",
            b"Content-Type: a\r\nContent-Type: b\r\n\r\n",
        ] {
            assert!(response(raw).is_err());
        }
    }
    pub fn task54_workflow_tls_v58_header_stream_and_count_caps_refuse_before_any_backend() {
        let mut many =
            String::from("CONNECT ghe.example:443 HTTP/1.1\r\nHost: ghe.example:443\r\n");
        for n in 0..64 {
            many.push_str(&format!("X-{n}: a\r\n"));
        }
        many.push_str("\r\n");
        assert!(connect(many.as_bytes()).is_err());
        let too_large = format!(
            "CONNECT ghe.example:443 HTTP/1.1\r\nHost: ghe.example:443\r\nUser-Agent: {}\r\n\r\n",
            "x".repeat(HEADER_CAP)
        );
        assert!(connect(too_large.as_bytes()).is_err());
        let response_too_large = vec![b'x'; STREAM_CAP + 1];
        assert!(response(&response_too_large).is_err());
        for raw in ["CONNECT ghe.example:443 HTTP/1.1\r\nHost: ghe.example:443\r\nUser-Agent: \u{1b}[2J\r\n\r\n", "CONNECT ghe.example:443 HTTP/1.1\nHost: ghe.example:443\n\n"] {assert!(connect(raw.as_bytes()).is_err());}
    }
}

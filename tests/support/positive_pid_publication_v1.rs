//! Fixture bytes are not a process identity. Require a complete publication
//! before callers perform their existing exact UID/birth/session ownership check.
use std::io;

pub fn complete_positive_pid(bytes: &[u8]) -> io::Result<Option<i32>> {
    if bytes.len() > 12 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Oversized fixture PID record",
        ));
    }
    let Some(record) = bytes.strip_suffix(b"\n") else {
        return Ok(None);
    };
    if record.is_empty() || !record.iter().all(u8::is_ascii_digit) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Fixture PID needs exactly one positive decimal newline record",
        ));
    }
    let pid = std::str::from_utf8(record)
        .ok()
        .and_then(|record| record.parse::<i32>().ok())
        .filter(|pid| *pid > 0)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "Fixture PID is outside the positive process-ID range",
            )
        })?;
    Ok(Some(pid))
}

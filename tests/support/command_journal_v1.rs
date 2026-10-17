//! JSONL is committed by its newline. An in-flight append is not a record.
//! Invalid committed UTF8 is an error; JSON validation remains with callers.
pub fn committed_prefix(bytes: &[u8]) -> Result<&str, std::str::Utf8Error> {
    let end = bytes
        .iter()
        .rposition(|byte| *byte == b'\n')
        .map_or(0, |i| i + 1);
    std::str::from_utf8(&bytes[..end])
}

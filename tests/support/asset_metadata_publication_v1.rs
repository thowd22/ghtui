//! A child PID is not publication of its later argv/environment observations.
//! Test-only completed-record parser; this grants no process or app authority.
pub fn complete_metadata(bytes: &[u8]) -> Result<bool, &'static str> {
    const RECORD: &[u8] = b"complete\n";
    if bytes == RECORD {
        Ok(true)
    } else if bytes.len() < RECORD.len() && RECORD.starts_with(bytes) {
        Ok(false)
    } else {
        Err("Asset metadata completion must be one exact bounded record")
    }
}

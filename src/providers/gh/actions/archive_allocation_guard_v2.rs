//! Allocation-envelope admission for the pinned ZIP2.4.2 parser, not an
//! extraction/content parser. The SAME ZipArchive then validates semantics.
//! Strict bounded profile: ordinary single-disk ZIP32, <=128KiB central table,
//! no alternate EOCD candidate or ZIP64, <=4096 records. Unsupported archives
//! are explicit unavailable, never silently partial or trusted/extracted.
use super::*;
const CENTRAL_LIMIT: usize = 128 * 1024;
fn u16at(bytes: &[u8], at: usize) -> GithubResult<usize> {
    Ok(u16::from_le_bytes(
        bytes
            .get(at..at + 2)
            .ok_or_else(|| bad("truncated ZIP envelope"))?
            .try_into()
            .map_err(|_| bad("truncated ZIP envelope"))?,
    ) as usize)
}
fn u32at(bytes: &[u8], at: usize) -> GithubResult<usize> {
    Ok(u32::from_le_bytes(
        bytes
            .get(at..at + 4)
            .ok_or_else(|| bad("truncated ZIP envelope"))?
            .try_into()
            .map_err(|_| bad("truncated ZIP envelope"))?,
    ) as usize)
}
fn bad(reason: &str) -> GithubError {
    error(GithubErrorKind::Unsupported,format!("Bounded Actions ZIP allocation profile unavailable: {reason}; no partial archive or extraction"))
}
pub(super) fn check(bytes: &Vec<u8>, cancel: &CancellationToken) -> GithubResult<()> {
    if bytes.len() < 22 || bytes.len() > 4 * 1024 * 1024 || bytes.capacity() > 8 * 1024 * 1024 {
        return Err(bad("input capacity/size"));
    }
    let mut footer = None;
    for (at, magic) in bytes.windows(4).enumerate() {
        if at % 256 == 0 && cancel.is_cancelled() {
            return Err(bad("original read canceled"));
        }
        if magic == b"PK\x06\x06" || magic == b"PK\x06\x07" {
            return Err(bad("ZIP64 is outside the bounded profile"));
        }
        if magic == b"PK\x05\x06" && footer.replace(at).is_some() {
            return Err(bad(
                "alternate EOCD candidates could trigger parser backtracking",
            ));
        }
    }
    let footer = footer.ok_or_else(|| bad("missing EOCD"))?;
    if footer + 22 > bytes.len()
        || footer + 22 + u16at(bytes, footer + 20)? != bytes.len()
        || u16at(bytes, footer + 4)? != 0
        || u16at(bytes, footer + 6)? != 0
    {
        return Err(bad("multi-disk/trailing/truncated footer"));
    }
    let count = u16at(bytes, footer + 10)?;
    let size = u32at(bytes, footer + 12)?;
    let start = u32at(bytes, footer + 16)?;
    if count > 4096
        || u16at(bytes, footer + 8)? != count
        || size > CENTRAL_LIMIT
        || start.checked_add(size) != Some(footer)
    {
        return Err(bad("count/central table extent"));
    }
    let mut at = start;
    // Conservative PINNED-parser allowance, not introspected process heap:
    // 2KiB/record covers Vec+IndexMap overlap, headers/keys and bounded public
    // entry/name-index nodes; names16x covers raw/CP437/lossy/key/path copies;
    // extras32x covers raw/stripped Vec overlap plus <=len/4 typed fields and
    // their growth.64KiB covers one local-header extra/comment/decompressor.
    let mut allocation = 64 * 1024usize + count * 2048;
    for _ in 0..count {
        if cancel.is_cancelled() {
            return Err(bad("original read canceled"));
        }
        if at.checked_add(46).is_none_or(|end| end > footer)
            || bytes.get(at..at + 4) != Some(b"PK\x01\x02")
        {
            return Err(bad("central record framing"));
        }
        let name = u16at(bytes, at + 28)?;
        let extra = u16at(bytes, at + 30)?;
        let comment = u16at(bytes, at + 32)?;
        allocation = allocation
            .saturating_add(name.saturating_mul(16))
            .saturating_add(extra.saturating_mul(32));
        if allocation > 8 * 1024 * 1024 {
            return Err(bad(
                "pinned-parser conservative allocation allowance exceeds8MiB",
            ));
        }
        if name == 0
            || name > 4096
            || extra > 256
            || comment != 0
            || u16at(bytes, at + 34)? != 0
            || u32at(bytes, at + 20)? == u32::MAX as usize
            || u32at(bytes, at + 24)? == u32::MAX as usize
            || u32at(bytes, at + 42)? == u32::MAX as usize
        {
            return Err(bad("record allocation/ZIP64/multi-disk profile"));
        }
        let end = at
            .checked_add(46)
            .and_then(|n| n.checked_add(name))
            .and_then(|n| n.checked_add(extra))
            .ok_or_else(|| bad("record overflow"))?;
        if end > footer {
            return Err(bad("central record exceeds admitted table"));
        }
        let mut field = at + 46 + name;
        while field < end {
            if field + 4 > end {
                return Err(bad("extra field framing"));
            }
            let tag = u16at(bytes, field)?;
            let len = u16at(bytes, field + 2)?;
            if tag == 1 {
                return Err(bad("ZIP64 extra field"));
            }
            field = field
                .checked_add(4 + len)
                .filter(|value| *value <= end)
                .ok_or_else(|| bad("extra field extent"))?;
        }
        at = end;
    }
    if at != footer {
        return Err(bad("central directory count/size disagrees"));
    }
    Ok(())
}
#[cfg(test)]
#[path = "archive_allocation_guard_v2_tests.rs"]
mod tests;

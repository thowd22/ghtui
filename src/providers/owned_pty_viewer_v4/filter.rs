//! Incremental, bounded escape allowlist. Raw child bytes never reach stdout.
use std::io;

const MAX_SEQUENCE: usize = 128;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FilterModeV4 {
    Capture,
    Pager,
}
enum State {
    Ground,
    Escape,
    Charset,
    Csi(Vec<u8>),
    String {
        escaped: bool,
        bytes: usize,
    },
    Utf8 {
        bytes: [u8; 4],
        len: usize,
        expected: usize,
    },
}
pub(crate) struct TerminalFilterV4 {
    mode: FilterModeV4,
    state: State,
    rows: u16,
    cols: u16,
}
impl TerminalFilterV4 {
    pub(crate) fn capture() -> Self {
        Self {
            mode: FilterModeV4::Capture,
            state: State::Ground,
            rows: 0,
            cols: 0,
        }
    }
    pub(crate) fn pager(rows: u16, cols: u16) -> io::Result<Self> {
        if !(4..=60).contains(&rows) || !(12..=240).contains(&cols) {
            return Err(invalid("Invalid bounded external filter geometry"));
        }
        Ok(Self {
            mode: FilterModeV4::Pager,
            state: State::Ground,
            rows,
            cols,
        })
    }
    pub(crate) fn feed(
        &mut self,
        bytes: &[u8],
        output: &mut Vec<u8>,
        cap: usize,
    ) -> io::Result<()> {
        for &byte in bytes {
            let state = std::mem::replace(&mut self.state, State::Ground);
            match state {
                State::Ground => self.ground(byte, output, cap)?,
                State::Escape => match byte {
                    b'[' => self.state = State::Csi(Vec::with_capacity(MAX_SEQUENCE)),
                    b']' | b'P' | b'^' | b'_' | b'X' => {
                        self.state = State::String {
                            escaped: false,
                            bytes: 0,
                        }
                    }
                    b'(' | b')' | b'*' | b'+' => self.state = State::Charset,
                    b'7' | b'8' | b'M' if self.mode == FilterModeV4::Pager => {
                        extend(output, &[0x1b, byte], cap)?
                    }
                    0x1b => self.state = State::Escape,
                    _ => {}
                },
                State::Charset => {}
                State::String { escaped, bytes } => {
                    if byte == 7 || (escaped && byte == b'\\') {
                        continue;
                    }
                    let bytes = bytes + 1;
                    if bytes > MAX_SEQUENCE {
                        return Err(invalid("external terminal string exceeds128 bytes"));
                    }
                    self.state = State::String {
                        escaped: byte == 0x1b,
                        bytes,
                    };
                }
                State::Csi(mut sequence) => {
                    if sequence.len() == MAX_SEQUENCE {
                        return Err(invalid("external CSI exceeds128 bytes"));
                    }
                    sequence.push(byte);
                    if (0x40..=0x7e).contains(&byte) {
                        if allowed_csi(self.mode, self.rows, self.cols, &sequence)? {
                            extend(output, b"\x1b[", cap)?;
                            extend(output, &sequence, cap)?;
                        }
                    } else if (0x20..=0x3f).contains(&byte) {
                        self.state = State::Csi(sequence);
                    } else if byte == 0x1b {
                        self.state = State::Escape;
                    } // Other controls abort the complete sequence.
                }
                State::Utf8 {
                    mut bytes,
                    len,
                    expected,
                } => {
                    if (0x80..=0xbf).contains(&byte) {
                        bytes[len] = byte;
                        if len + 1 == expected {
                            let text = std::str::from_utf8(&bytes[..expected])
                                .map_err(|_| invalid("invalid external UTF-8"))?;
                            for ch in text.chars() {
                                safe_char(ch, output, cap)?;
                            }
                        } else {
                            self.state = State::Utf8 {
                                bytes,
                                len: len + 1,
                                expected,
                            };
                        }
                    } else {
                        return Err(invalid("invalid external UTF-8 continuation"));
                    }
                }
            }
        }
        Ok(())
    }
    fn ground(&mut self, byte: u8, output: &mut Vec<u8>, cap: usize) -> io::Result<()> {
        match byte {
            0x1b => self.state = State::Escape,
            b'\n' | b'\t' | 0x20..=0x7e => extend(output, &[byte], cap)?,
            b'\r' | 8 if self.mode == FilterModeV4::Pager => extend(output, &[byte], cap)?,
            b'\r' => extend(output, b"\\r", cap)?,
            8 => extend(output, b"\\b", cap)?,
            0xc2..=0xf4 => {
                let mut bytes = [0; 4];
                bytes[0] = byte;
                self.state = State::Utf8 {
                    bytes,
                    len: 1,
                    expected: if byte < 0xe0 {
                        2
                    } else if byte < 0xf0 {
                        3
                    } else {
                        4
                    },
                };
            }
            0x80..=0xff => return Err(invalid("invalid external UTF-8 lead")),
            _ => {} // bell, ENQ, DEL and other C0 are not host effects.
        }
        Ok(())
    }
    pub(crate) fn finish(&self) -> io::Result<()> {
        if matches!(self.state, State::Ground) {
            Ok(())
        } else {
            Err(invalid(
                "external output ended inside an escape/UTF-8 sequence",
            ))
        }
    }
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
pub(super) fn extend(output: &mut Vec<u8>, bytes: &[u8], cap: usize) -> io::Result<()> {
    let next = output
        .len()
        .checked_add(bytes.len())
        .filter(|n| *n <= cap)
        .ok_or_else(|| invalid("external sanitized output exceeds byte bound"))?;
    if next > output.capacity() {
        let target = next
            .max(output.capacity().saturating_mul(2))
            .max(256)
            .min(cap);
        output
            .try_reserve_exact(target.saturating_sub(output.len()))
            .map_err(|_| invalid("external allocation refused"))?;
        if output.capacity() > cap {
            return Err(invalid("external retained capacity exceeds byte bound"));
        }
    }
    output.extend_from_slice(bytes);
    Ok(())
}
fn unsafe_char(ch: char) -> bool {
    ch.is_control()
        || matches!(ch, '\u{061c}'|'\u{200e}'|'\u{200f}'|'\u{202a}'..='\u{202e}'|'\u{2066}'..='\u{2069}')
}
fn safe_char(ch: char, output: &mut Vec<u8>, cap: usize) -> io::Result<()> {
    if unsafe_char(ch) {
        let text = format!("\\u{{{:x}}}", ch as u32);
        extend(output, text.as_bytes(), cap)
    } else {
        let mut bytes = [0; 4];
        extend(output, ch.encode_utf8(&mut bytes).as_bytes(), cap)
    }
}
pub(super) fn safe_cell(text: &str) -> String {
    let mut result = String::with_capacity(48);
    for ch in text.chars().take(6) {
        if unsafe_char(ch) {
            result.push('\u{fffd}');
        } else {
            result.push(ch);
        }
    }
    result
}
fn parameters(bytes: &[u8]) -> Option<Vec<u16>> {
    if bytes.len() > MAX_SEQUENCE || bytes.iter().any(|b| !b.is_ascii_digit() && *b != b';') {
        return None;
    }
    let mut result = Vec::with_capacity(32);
    for word in bytes.split(|b| *b == b';') {
        if word.len() > 5 || result.len() == 32 {
            return None;
        }
        let text = std::str::from_utf8(word).ok()?;
        result.push(if word.is_empty() {
            0
        } else {
            text.parse::<u16>().ok()?
        });
    }
    Some(result)
}
fn sgr(params: &[u16]) -> bool {
    let mut index = 0;
    while index < params.len() {
        match params[index] {
            0..=4 | 7 | 22..=24 | 27 | 30..=37 | 39 | 40..=47 | 49 | 90..=97 | 100..=107 => {
                index += 1
            }
            38 | 48 => match params.get(index + 1) {
                Some(5) if params.get(index + 2).is_some_and(|n| *n <= 255) => index += 3,
                Some(2)
                    if params
                        .get(index + 2..index + 5)
                        .is_some_and(|rgb| rgb.iter().all(|n| *n <= 255)) =>
                {
                    index += 5
                }
                _ => return false,
            },
            _ => return false,
        }
    }
    true
}
fn allowed_csi(mode: FilterModeV4, rows: u16, cols: u16, sequence: &[u8]) -> io::Result<bool> {
    let Some((&last, raw)) = sequence.split_last() else {
        return Ok(false);
    };
    if last == b'm' {
        return Ok(parameters(raw).is_some_and(|p| sgr(&p)));
    }
    if mode == FilterModeV4::Capture {
        return Ok(false);
    }
    if matches!(last, b'h' | b'l') {
        let Some(raw) = raw.strip_prefix(b"?") else {
            return Ok(false);
        };
        // Only modes implemented by this pinned vt100 may reach its two fixed
        // grids. Mouse, paste, query and unimplemented modes remain stripped.
        return Ok(
            parameters(raw).is_some_and(|p| p.iter().all(|n| matches!(n, 1 | 6 | 25 | 47 | 1049)))
        );
    }
    if !matches!(
        last,
        b'@' | b'P'
            | b'X'
            | b'C'
            | b'D'
            | b'G'
            | b'A'
            | b'B'
            | b'd'
            | b'L'
            | b'M'
            | b'S'
            | b'T'
            | b'H'
            | b'r'
            | b'J'
            | b'K'
    ) {
        return Ok(false);
    }
    let Some(params) = parameters(raw) else {
        return Err(invalid(
            "External cursor command has unsupported parameters",
        ));
    };
    let bounded = match last {
        // Pinned Grid::insert_cells grows BEFORE truncation. A generic u16
        // parameter bound is NOT a grid allocation bound: cap by real width.
        // Its canonicalizers turn an absent/zero movement into 1, while r
        // defaults to (1, rows); all remain inside our actual >=4x12 geometry.
        b'@' | b'P' | b'X' | b'C' | b'D' | b'G' => params.len() == 1 && params[0] <= cols,
        b'A' | b'B' | b'd' | b'L' | b'M' | b'S' | b'T' => params.len() == 1 && params[0] <= rows,
        b'H' => params.len() <= 2 && params[0] <= rows && params.get(1).is_none_or(|n| *n <= cols),
        b'r' => params.len() <= 2 && params.iter().all(|n| *n <= rows),
        b'J' | b'K' => params.len() == 1 && params[0] <= 2,
        _ => false,
    };
    if bounded {
        Ok(true)
    } else {
        // Do not silently omit an edit and then label the external view as a
        // complete emulation. The host still owns Return after this refusal.
        Err(invalid(
            "External cursor command exceeds actual pane geometry",
        ))
    }
}

#[cfg(test)]
#[path = "filter_tests.rs"]
mod tests;

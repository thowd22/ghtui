//! Test-only controlled consumer latency. Actual PTY bytes remain in the SAME
//! receiver until the one bounded hold ends; no frame or input is fabricated.
use std::time::{Duration, Instant};
pub const MAX_HOLD: Duration = Duration::from_millis(200);
const MAX_WRITES: usize = 128;
const MAX_INPUT: usize = 64;
const MAX_CHUNK: usize = 8192;
const MAX_LITERAL: usize = 128;
struct LiteralHoldV1 {
    needle: Vec<u8>,
    suffix: Vec<u8>,
    pending: Vec<u8>,
    matched: bool,
}
#[derive(Debug)]
pub struct ActualWriteV1 {
    pub bytes: Vec<u8>,
    pub completed_cells_end: usize,
}
pub struct ReceiverDeliveryGateV1 {
    duration: Duration,
    original_completed_end: usize,
    release_at: Option<Instant>,
    writes: Vec<ActualWriteV1>,
    literal: Option<LiteralHoldV1>,
}
impl ReceiverDeliveryGateV1 {
    pub fn new(duration: Duration, completed_end: usize) -> Result<Self, &'static str> {
        if duration.is_zero() || duration > MAX_HOLD || completed_end == 0 {
            return Err("Hold requires a real completed frame and 1..=200ms");
        }
        Ok(Self {
            duration,
            original_completed_end: completed_end,
            release_at: None,
            writes: Vec::new(),
            literal: None,
        })
    }
    pub fn after_literal(needle: &[u8], completed_end: usize) -> Result<Self, &'static str> {
        if needle.is_empty() || needle.len() > MAX_LITERAL {
            return Err("Actual literal must contain 1..=128 bytes");
        }
        let mut gate = Self::new(MAX_HOLD, completed_end)?;
        gate.literal = Some(LiteralHoldV1 {
            needle: needle.to_vec(),
            suffix: Vec::new(),
            pending: Vec::new(),
            matched: false,
        });
        Ok(gate)
    }
    /// Called ONLY after actual PTY write_all AND flush succeed.
    pub fn flushed(
        &mut self,
        bytes: &[u8],
        completed_end: usize,
        now: Instant,
    ) -> Result<(), &'static str> {
        if bytes.is_empty()
            || bytes.len() > MAX_INPUT
            || self.writes.len() >= MAX_WRITES
            || completed_end < self.original_completed_end
        {
            return Err("Actual-input journal bounds or completed endpoint refused");
        }
        if self.literal.is_none() {
            self.release_at.get_or_insert(now + self.duration);
        }
        self.writes.push(ActualWriteV1 {
            bytes: bytes.to_vec(),
            completed_cells_end: completed_end,
        });
        Ok(())
    }
    pub fn remaining(&self, now: Instant) -> Option<Duration> {
        self.release_at
            .and_then(|deadline| deadline.checked_duration_since(now))
            .filter(|duration| !duration.is_zero())
    }
    pub fn writes(&self) -> &[ActualWriteV1] {
        &self.writes
    }
    /// Only a literal contiguous sequence of ACTUAL receiver bytes triggers
    /// this seam. Packet splits are preserved, never interpreted as VT state.
    pub fn receiver_chunk(&mut self, bytes: &[u8], now: Instant) -> Result<usize, &'static str> {
        let Some(literal) = &mut self.literal else {
            return Ok(bytes.len());
        };
        if bytes.len() > MAX_CHUNK {
            return Err("Original PTY reader chunk cap exceeded");
        }
        if literal.matched {
            return if literal.pending.is_empty() {
                Ok(bytes.len())
            } else {
                Err("Held original tail must be released before later receiver bytes")
            };
        }
        let previous = literal.suffix.len();
        let mut observed = literal.suffix.clone();
        observed.extend_from_slice(bytes);
        if let Some(index) = observed
            .windows(literal.needle.len())
            .position(|part| part == literal.needle)
        {
            let delivered = index + literal.needle.len() - previous;
            literal.pending.extend_from_slice(&bytes[delivered..]);
            literal.matched = true;
            self.release_at = Some(now + self.duration);
            return Ok(delivered);
        }
        let keep = (literal.needle.len() - 1).min(observed.len());
        literal.suffix = observed[observed.len() - keep..].to_vec();
        Ok(bytes.len())
    }
    pub fn release_tail(&mut self, now: Instant) -> Option<Vec<u8>> {
        if self.remaining(now).is_some() {
            return None;
        }
        let literal = self.literal.as_mut()?;
        literal
            .matched
            .then(|| std::mem::take(&mut literal.pending))
            .filter(|bytes| !bytes.is_empty())
    }
    pub fn ensure_triggered(&self) -> Result<(), &'static str> {
        if self.release_at.is_none() {
            Err("Controlled setup never observed the genuine contiguous literal/write")
        } else {
            Ok(())
        }
    }
    pub fn into_writes(self) -> Vec<ActualWriteV1> {
        self.writes
    }
}
pub fn maximum_unacknowledged(writes: &[ActualWriteV1]) -> usize {
    let mut previous = None;
    let mut current = 0;
    let mut maximum = 0;
    for write in writes {
        current = if previous == Some(write.completed_cells_end) {
            current + 1
        } else {
            1
        };
        previous = Some(write.completed_cells_end);
        maximum = maximum.max(current);
    }
    maximum
}

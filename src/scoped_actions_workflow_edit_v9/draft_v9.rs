//! Text recovery only. The canonical frames carry no source, pathname,
//! permission, receipt, approval or executable operation.
use super::{COMMIT_MESSAGE_BYTES_V9, INPUT_BYTES_V9, PR_BODY_BYTES_V9, YAML_BYTES_V9};

pub(crate) const WORKFLOW_DRAFT_BYTES_V9: usize = 361 * 1024;
const MAGIC: &str = "ghtui-workflow-edit-draft-v9\n";
const LIMITS: [usize; 6] = [
    8,
    YAML_BYTES_V9,
    256,
    COMMIT_MESSAGE_BYTES_V9,
    1024,
    PR_BODY_BYTES_V9,
];

pub(crate) struct WorkflowDraftTextV9<'a> {
    rows: [&'a str; 6],
}
impl std::fmt::Debug for WorkflowDraftTextV9<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("WorkflowDraftTextV9(<private editable text only; no source/approval>)")
    }
}
impl<'a> WorkflowDraftTextV9<'a> {
    pub(crate) fn from_fields(rows: [&'a str; 6]) -> Result<Self, String> {
        let total = rows[1..]
            .iter()
            .try_fold(0usize, |sum, text| sum.checked_add(text.len()));
        if !matches!(rows[0], "isolated" | "direct")
            || total.is_none_or(|n| n > INPUT_BYTES_V9)
            || rows
                .iter()
                .zip(LIMITS)
                .any(|(text, limit)| text.len() > limit || text.contains('\0'))
        {
            return Err(refusal());
        }
        Ok(Self { rows })
    }
    pub(crate) fn rows(&self) -> [&'a str; 6] {
        self.rows
    }
    pub(crate) fn encoded_len(&self) -> Result<usize, String> {
        self.rows.iter().try_fold(MAGIC.len(), |size, row| {
            let digits = if row.is_empty() {
                1
            } else {
                row.len().ilog10() as usize + 1
            };
            size.checked_add(digits + 1)
                .and_then(|n| n.checked_add(row.len()))
                .filter(|n| *n <= WORKFLOW_DRAFT_BYTES_V9)
                .ok_or_else(refusal)
        })
    }
    pub(crate) fn encode(&self) -> Result<String, String> {
        use std::fmt::Write;
        let expected = self.encoded_len()?; // Borrowed preflight BEFORE allocation/copy.
        let mut body = String::with_capacity(expected);
        body.push_str(MAGIC);
        for row in self.rows {
            write!(&mut body, "{}:", row.len()).map_err(|_| refusal())?;
            body.push_str(row);
        }
        if body.len() != expected || body.capacity() > WORKFLOW_DRAFT_BYTES_V9 {
            return Err(refusal());
        }
        Ok(body)
    }
    /// Borrowed decoding. Even malformed final fields cannot allocate earlier
    /// private fields. Only a subsequent genuine worker opener may copy text.
    pub(crate) fn decode(body: &'a str) -> Result<Self, String> {
        if body.len() > WORKFLOW_DRAFT_BYTES_V9 || !body.starts_with(MAGIC) {
            return Err(refusal());
        }
        let mut remaining = &body[MAGIC.len()..];
        let mut rows = [""; 6];
        for (slot, limit) in rows.iter_mut().zip(LIMITS) {
            let colon = remaining
                .as_bytes()
                .iter()
                .take(8)
                .position(|b| *b == b':')
                .ok_or_else(refusal)?;
            let digits = &remaining.as_bytes()[..colon];
            if digits.is_empty() || (digits.len() > 1 && digits[0] == b'0') {
                return Err(refusal());
            }
            let length = digits
                .iter()
                .try_fold(0usize, |n, digit| {
                    if !digit.is_ascii_digit() {
                        return None;
                    }
                    n.checked_mul(10)?.checked_add((digit - b'0') as usize)
                })
                .filter(|n| *n <= limit)
                .ok_or_else(refusal)?;
            remaining = &remaining[colon + 1..];
            *slot = remaining.get(..length).ok_or_else(refusal)?;
            remaining = remaining.get(length..).ok_or_else(refusal)?;
        }
        if !remaining.is_empty() {
            return Err(refusal());
        }
        Self::from_fields(rows)
    }
}
fn refusal() -> String {
    "Original private workflow draft is malformed or exceeds its fixed byte bounds; no fields/source/approval restored".into()
}

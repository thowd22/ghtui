//! Exact prose only, from ONE already completed and owned body. This is not a
//! screen/action/coordinate parser and cannot join separate frames or hidden rows.
/// This Workflow-only readiness oracle never treats retained busy preview
/// content or an already-journaled final child as an admitted current Form.
pub(super) fn ready_workflow_body_v124(body: &str, required: &[&str]) -> bool {
    body.contains("[Keep draft / Close]")
        && !body.contains("[Esc Cancel]")
        && required.iter().all(|word| {
            if matches!(*word, "no PUT/resend" | "state disabled_manually") {
                adjacent_literal(body, word)
            } else {
                body.contains(word)
            }
        })
}
pub(super) fn adjacent_literal(body: &str, literal: &str) -> bool {
    if literal.is_empty()
        || literal.len() > 1024
        || literal.contains(['\n', '\r'])
        || body.len() > 128 * 1024
    {
        return false;
    }
    let rows: Vec<_> = body.lines().take(65).collect();
    if rows.len() > 64 {
        return false;
    }
    fn continues(rows: &[&str], remaining: &str) -> bool {
        let Some((row, rest)) = rows.split_first() else {
            return remaining.is_empty();
        };
        // Only padding at an actual physical row boundary is removed. Blank
        // rows terminate adjacency; every interior byte is left untouched.
        let row = row.trim_matches(' ');
        if row.is_empty() {
            return false;
        }
        if row.starts_with(remaining) {
            return true;
        }
        let Some(remaining) = remaining.strip_prefix(row) else {
            return false;
        };
        continues(rest, remaining)
            || remaining
                .strip_prefix(' ')
                .is_some_and(|after_space| continues(rest, after_space))
    }
    for (index, row) in rows.iter().enumerate() {
        let row = row.trim_end_matches(' ');
        for (start, _) in row.char_indices() {
            let suffix = &row[start..];
            if suffix.starts_with(literal) {
                return true;
            }
            if let Some(remaining) = literal.strip_prefix(suffix) {
                if continues(&rows[index + 1..], remaining)
                    || remaining
                        .strip_prefix(' ')
                        .is_some_and(|after_space| continues(&rows[index + 1..], after_space))
                {
                    return true;
                }
            }
        }
    }
    false
}

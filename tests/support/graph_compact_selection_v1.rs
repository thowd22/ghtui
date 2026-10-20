//! Test-only navigation hints for the independently generated linear10000 DAG.
//! Full saved SHA/Git effects remain separate native oracles, not these labels.
pub struct DownBatchV1 {
    pub count: usize,
    pub expected_subject: String,
}
impl DownBatchV1 {
    pub fn bytes(&self) -> Vec<u8> {
        b"\x1b[B".repeat(self.count)
    }
}
pub fn selected_subject(frame: &vt100::Screen) -> Result<String, String> {
    let contents = frame.contents();
    let mut selected = contents.lines().enumerate().filter_map(|(row, line)| {
        // A reversed row, or the History row that carries the `▌` selection bar.
        (line.starts_with('│')
            && frame
                .cell(row as u16, 1)
                .is_some_and(|cell| cell.inverse() || cell.contents() == "▌"))
        .then_some(line)
    });
    let line = selected.next().ok_or("No actual inverted graph body row")?;
    if selected.next().is_some() {
        return Err("Ambiguous multiple inverted graph body rows".into());
    }
    if line.matches("Ordinary graph version ").count() != 1 {
        return Err("Ambiguous ordinary subject in the inverted graph row".into());
    }
    let start = line
        .find("Ordinary graph version ")
        .ok_or("Inverted row is not an ordinary commit; never guess across parent/control rows")?;
    let tail = &line[start..];
    let number = subject_number(tail)?;
    Ok(format!("Ordinary graph version {number:05}"))
}
fn subject_number(subject: &str) -> Result<usize, String> {
    let suffix = subject
        .strip_prefix("Ordinary graph version ")
        .ok_or("Not an exact ordinary graph subject")?;
    let digits = suffix.get(..5).ok_or("Incomplete ordinary graph subject")?;
    if !digits.bytes().all(|byte| byte.is_ascii_digit())
        || suffix.get(5..).is_some_and(|rest| {
            !rest.is_empty() && !rest.starts_with(" •") && !rest.starts_with(" (")
        })
    {
        return Err("Ambiguous or malformed ordinary graph subject".into());
    }
    digits
        .parse()
        .map_err(|_| "Invalid ordinary graph ordinal".into())
}
/// At most8 commit transitions, with EACH genuine parent connector preserved.
/// This deliberately accepts only the actual generated first200 projection.
pub fn first_page_down_batch(
    frame: &vt100::Screen,
    target: &str,
    actual_saved_sha: &str,
) -> Result<Option<DownBatchV1>, String> {
    let screen = frame.contents();
    // The key bar is the last row; its tokens carry no brackets.
    let key_bar = frame.rows(0, frame.size().1).last().unwrap_or_default();
    let mut saved = screen.lines().filter_map(|line| {
        line.trim_start_matches('│')
            .trim_start()
            .strip_prefix("Saved: ")
            .map(|tail| tail.split_whitespace().next().unwrap_or_default())
    });
    let exact_saved = saved.next() == Some(actual_saved_sha) && saved.next().is_none();
    if !matches!(actual_saved_sha.len(), 40 | 64)
        || !actual_saved_sha
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
        || !exact_saved
        || !(screen.contains("[Home First loaded]") || key_bar.contains("Home First loaded"))
        || screen.contains("[Esc Cancel]")
        || key_bar.contains("Esc Cancel")
    {
        return Err("Not the exact completed original graph source/projection".into());
    }
    let current = subject_number(&selected_subject(frame)?)?;
    let target = subject_number(target)?;
    if !(9800..=9999).contains(&current) || !(9800..=9999).contains(&target) {
        return Err("Subject is outside the actual ordinary first200 projection".into());
    }
    if target > current {
        return Err("Down would move away from the exact target; no compensating seek".into());
    }
    if target == current {
        return Ok(None);
    }
    let commits = (current - target).min(8);
    Ok(Some(DownBatchV1 {
        // Linear commits take one row each: a single straight parent is shown
        // by its lane, not by a separate connector row.
        count: commits,
        expected_subject: format!("Ordinary graph version {:05}", current - commits),
    }))
}
